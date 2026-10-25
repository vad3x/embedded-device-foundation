//! ESP-IDF OTA Bootloader State Machine
//!
//! Manages the `otadata` partition containing two redundant `esp_ota_select_entry_t`
//! structures. Tracks sequence numbers, image validation states (`ESP_OTA_IMG_NEW`,
//! `ESP_OTA_IMG_VALID`, `ESP_OTA_IMG_INVALID`), CRC-32 integrity, active slot
//! resolution, and fail-safe rollback logic across `ota_0` and `ota_1` slots.

use log::{debug, info};
#[cfg(target_arch = "riscv32")]
use log::warn;

// ============================================================================
// Public Structs & Implementation
// ============================================================================

/// ESP-IDF compatible OTA bootloader coordinator.
///
/// Tracks the active running slot, resolves next target slots, and commits
/// validation / pending verification states into redundant `otadata` sectors.
pub struct OtaBootloader<F = DefaultFlash> {
    flash: F,
    active: ActiveSlot,
}

impl<F: OtaFlashStorage> OtaBootloader<F> {
    /// Initialize the bootloader by reading `otadata` sectors from flash storage.
    pub fn new(mut flash: F) -> Result<Self, BootloaderError> {
        let mut sector0 = [0xffu8; OTA_DATA_ENTRY_SIZE];
        let mut sector1 = [0xffu8; OTA_DATA_ENTRY_SIZE];
        flash
            .read_otadata(&mut sector0, &mut sector1)
            .map_err(|_| BootloaderError::Flash)?;

        let active = resolve_active_slot(&sector0, &sector1);
        debug!(
            "OTA Bootloader initialized: active slot={:?}, seq={}, state={:?}",
            active.slot, active.seq, active.state
        );

        Ok(Self { flash, active })
    }

    /// Returns the currently running application partition slot.
    pub fn get_running_slot(&self) -> Slot {
        self.active.slot
    }

    /// Returns the current active sequence number in `otadata`.
    pub fn get_running_seq(&self) -> u32 {
        self.active.seq
    }

    /// Returns the target partition slot for the next firmware update.
    pub fn get_target_slot(&self) -> Slot {
        self.active.slot.other()
    }

    /// Returns the sequence number that will be assigned to the next firmware update.
    pub fn get_next_seq(&self) -> u32 {
        compute_next_seq(self.active.seq, self.get_target_slot())
    }

    /// Returns the full active slot metadata.
    pub fn get_active(&self) -> ActiveSlot {
        self.active
    }

    /// Commit a newly flashed image into `otadata` marked as `OtaState::New`.
    ///
    /// The entry is written to the alternate sector with the calculated sequence number,
    /// so the ESP-IDF 2nd-stage bootloader selects it on the next system reset.
    pub fn mark_pending_verify(
        &mut self,
        target_slot: Slot,
        seq: u32,
    ) -> Result<(), BootloaderError> {
        if seq == 0 || seq == u32::MAX || seq <= self.active.seq {
            return Err(BootloaderError::InvalidSequence);
        }
        if target_slot != self.active.slot.other() || Slot::from_seq(seq) != target_slot {
            return Err(BootloaderError::InvalidSlot);
        }

        let write_sec = if self.active.seq == 0 {
            0
        } else {
            1 - self.active.sector_index
        };

        let mut sector = [0xffu8; OTA_DATA_ENTRY_SIZE];
        write_ota_entry(&mut sector, seq, OtaState::New);

        self.flash
            .write_otadata_sector(write_sec, &sector)
            .map_err(|_| BootloaderError::Flash)?;

        info!(
            "OTA Bootloader: marked sector {} pending verify (slot={:?}, seq={})",
            write_sec, target_slot, seq
        );

        self.active = ActiveSlot {
            slot: target_slot,
            seq,
            state: OtaState::New,
            sector_index: write_sec,
        };

        Ok(())
    }

    /// Confirm that the currently running firmware has booted and verified successfully.
    ///
    /// If the active entry is not already `OtaState::Valid`, rewrites the active sector
    /// with `OtaState::Valid` to cancel automatic bootloader rollback.
    pub fn mark_valid(&mut self) -> Result<(), BootloaderError> {
        if self.active.state == OtaState::Valid && self.active.seq > 0 {
            return Ok(());
        }

        let write_sec = self.active.sector_index;
        let seq = if self.active.seq == 0 { 1 } else { self.active.seq };
        // Confirmation retains the current sequence rather than advancing it.
        if seq == 0 || seq == u32::MAX {
            return Err(BootloaderError::InvalidSequence);
        }
        if Slot::from_seq(seq) != self.active.slot {
            return Err(BootloaderError::InvalidSlot);
        }

        let mut sector = [0xffu8; OTA_DATA_ENTRY_SIZE];
        write_ota_entry(&mut sector, seq, OtaState::Valid);

        self.flash
            .write_otadata_sector(write_sec, &sector)
            .map_err(|_| BootloaderError::Flash)?;

        info!(
            "OTA Bootloader: confirmed valid slot={:?}, seq={} in sector {}",
            self.active.slot, seq, write_sec
        );

        self.active = ActiveSlot {
            slot: Slot::from_seq(seq),
            seq,
            state: OtaState::Valid,
            sector_index: write_sec,
        };

        Ok(())
    }
}

#[cfg(target_arch = "riscv32")]
impl OtaBootloader<EspOtaFlash> {
    /// Load bootloader state using the ESP32-C6 ROM SPI flash driver.
    pub fn load() -> Result<Self, BootloaderError> {
        Self::new(EspOtaFlash)
    }
}

// ============================================================================
// Public Functions
// ============================================================================

/// Determine the active application slot from two raw `otadata` sectors.
///
/// Evaluates CRC-32 integrity and checks that the image state is not `Invalid`
/// or `Aborted`. Whichever sector contains the highest valid sequence number
/// wins. If both sectors are blank or invalid, defaults to `Slot::Ota0` (seq 0).
pub fn resolve_active_slot(sector0: &[u8], sector1: &[u8]) -> ActiveSlot {
    let entry0 = EspOtaSelectEntry::from_bytes(sector0);
    let entry1 = EspOtaSelectEntry::from_bytes(sector1);

    let valid0 = entry0.as_ref().map_or(false, |e| e.is_valid());
    let valid1 = entry1.as_ref().map_or(false, |e| e.is_valid());

    match (valid0, valid1) {
        (true, true) => {
            let e0 = entry0.unwrap();
            let e1 = entry1.unwrap();
            if e0.ota_seq > e1.ota_seq {
                ActiveSlot {
                    slot: Slot::from_seq(e0.ota_seq),
                    seq: e0.ota_seq,
                    state: OtaState::from_u32(e0.ota_state),
                    sector_index: 0,
                }
            } else {
                ActiveSlot {
                    slot: Slot::from_seq(e1.ota_seq),
                    seq: e1.ota_seq,
                    state: OtaState::from_u32(e1.ota_state),
                    sector_index: 1,
                }
            }
        }
        (true, false) => {
            let e0 = entry0.unwrap();
            ActiveSlot {
                slot: Slot::from_seq(e0.ota_seq),
                seq: e0.ota_seq,
                state: OtaState::from_u32(e0.ota_state),
                sector_index: 0,
            }
        }
        (false, true) => {
            let e1 = entry1.unwrap();
            ActiveSlot {
                slot: Slot::from_seq(e1.ota_seq),
                seq: e1.ota_seq,
                state: OtaState::from_u32(e1.ota_state),
                sector_index: 1,
            }
        }
        (false, false) => ActiveSlot {
            slot: Slot::Ota0,
            seq: 0,
            state: OtaState::Undefined,
            sector_index: 0,
        },
    }
}

/// Calculate the opposite slot for alternate ping-pong flashing.
pub fn compute_target_slot(active_slot: Slot) -> Slot {
    active_slot.other()
}

/// Calculate the smallest sequence number strictly greater than `current_seq`
/// that maps to `target_slot` under the ESP-IDF formula: `(seq - 1) % 2 == slot_idx`.
///
/// Employs `wrapping_add` and a loop guard to avoid infinite loops if `current_seq`
/// is near or at `u32::MAX`. An exhausted counter can yield a reserved or lower
/// sequence; `OtaBootloader::mark_pending_verify` rejects those before writing.
pub fn compute_next_seq(current_seq: u32, target_slot: Slot) -> u32 {
    let target_idx = target_slot.index();
    let mut next_seq = current_seq.wrapping_add(1);
    let mut attempts = 0;
    while attempts < 4 && (next_seq == 0 || (next_seq - 1) % 2 != target_idx) {
        next_seq = next_seq.wrapping_add(1);
        attempts += 1;
    }
    if next_seq == 0 || (next_seq - 1) % 2 != target_idx {
        if target_idx == 0 {
            1
        } else {
            2
        }
    } else {
        next_seq
    }
}

/// Compute the ESP-IDF compatible CRC-32 checksum of an OTA sequence number.
///
/// Matches ESP-IDF's `bootloader_common_ota_select_crc()`:
/// ```c
/// uint32_t bootloader_common_ota_select_crc(const esp_ota_select_entry_t *s) {
///     return crc32_le(UINT32_MAX, (uint8_t*)&s->ota_seq, 4);
/// }
/// ```
/// Note: ESP-IDF's `crc32_le(UINT32_MAX, ...)` starts by inverting the seed
/// `!UINT32_MAX` (= 0), processing data with the little-endian polynomial
/// `0xEDB88320`, and inverting the output.
pub fn compute_crc(ota_seq: u32) -> u32 {
    esp_crc32_le(u32::MAX, &ota_seq.to_le_bytes())
}

/// Calculate little-endian CRC-32 matching ESP-IDF's `crc32_le` ROM function.
pub fn esp_crc32_le(crc: u32, buf: &[u8]) -> u32 {
    let mut crc = !crc;
    for &byte in buf {
        crc = CRC32_LE_TABLE[(crc as usize ^ byte as usize) & 0xff] ^ (crc >> 8);
    }
    !crc
}

/// Serialize an `esp_ota_select_entry_t` into the beginning of a sector buffer.
pub fn write_ota_entry(buffer: &mut [u8], seq: u32, state: OtaState) {
    let entry = EspOtaSelectEntry::new(seq, state);
    let bytes = entry.to_bytes();
    if buffer.len() >= EspOtaSelectEntry::SIZE {
        buffer[..EspOtaSelectEntry::SIZE].copy_from_slice(&bytes);
    }
}

/// Standalone convenience helper called on startup to confirm running firmware health.
#[cfg(target_arch = "riscv32")]
pub fn mark_running_slot_valid() -> Result<(), BootloaderError> {
    let mut bootloader = OtaBootloader::load()?;
    bootloader.mark_valid()
}

/// Standalone convenience helper on host targets (no-op success).
#[cfg(not(target_arch = "riscv32"))]
pub fn mark_running_slot_valid() -> Result<(), BootloaderError> {
    Ok(())
}

// ============================================================================
// Public Supporting Types & Traits
// ============================================================================

/// NOR Flash abstraction for reading and writing `otadata` sectors.
pub trait OtaFlashStorage {
    type Error: core::fmt::Debug;

    /// Read both 32-byte `otadata` entries from offset 0 of each sector.
    fn read_otadata(
        &mut self,
        sector0: &mut [u8; OTA_DATA_ENTRY_SIZE],
        sector1: &mut [u8; OTA_DATA_ENTRY_SIZE],
    ) -> Result<(), Self::Error>;

    /// Erase and rewrite one `otadata` sector (0 or 1) with an updated 32-byte entry.
    fn write_otadata_sector(
        &mut self,
        sector_index: usize,
        data: &[u8; OTA_DATA_ENTRY_SIZE],
    ) -> Result<(), Self::Error>;
}

/// Information describing the currently active boot slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveSlot {
    /// Active partition slot (`Ota0` or `Ota1`).
    pub slot: Slot,
    /// OTA sequence counter.
    pub seq: u32,
    /// Image verification state.
    pub state: OtaState,
    /// Sector index in `otadata` that hosts this entry (0 or 1).
    pub sector_index: usize,
}

/// Dual-bank firmware partition slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// Primary slot (`ota_0`).
    Ota0,
    /// Secondary slot (`ota_1`).
    Ota1,
}

impl Slot {
    /// Return the starting byte offset of this slot in flash.
    pub fn flash_offset(&self) -> u32 {
        match self {
            Slot::Ota0 => 0x020000,
            Slot::Ota1 => 0x370000,
        }
    }

    /// Return the alternate slot.
    pub fn other(&self) -> Self {
        match self {
            Slot::Ota0 => Slot::Ota1,
            Slot::Ota1 => Slot::Ota0,
        }
    }

    /// Return string representation matching `partitions.csv`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Slot::Ota0 => "ota_0",
            Slot::Ota1 => "ota_1",
        }
    }

    /// Partition index (0 for `ota_0`, 1 for `ota_1`).
    pub fn index(&self) -> u32 {
        match self {
            Slot::Ota0 => 0,
            Slot::Ota1 => 1,
        }
    }

    /// Create from zero-based partition index.
    pub fn from_index(index: u32) -> Option<Self> {
        match index {
            0 => Some(Slot::Ota0),
            1 => Some(Slot::Ota1),
            _ => None,
        }
    }

    /// Map sequence counter to partition slot using ESP-IDF formula `(seq - 1) % 2`.
    pub fn from_seq(seq: u32) -> Self {
        if seq == 0 || (seq.saturating_sub(1)) % 2 == 0 {
            Slot::Ota0
        } else {
            Slot::Ota1
        }
    }
}

/// ESP-IDF image states stored in `esp_ota_select_entry_t.ota_state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum OtaState {
    /// Newly written image awaiting first boot.
    New = 0x00000000,
    /// Running first trial boot, pending confirmation.
    PendingVerify = 0x00000001,
    /// Verified functional; rollback restrictions cancelled.
    Valid = 0x00000002,
    /// Failed diagnostics; bootloader will roll back.
    Invalid = 0x00000003,
    /// Reset during trial before verification; bootloader rolls back.
    Aborted = 0x00000004,
    /// Default erased flash state (all 0xFF).
    Undefined = 0xFFFFFFFF,
}

impl OtaState {
    /// Convert raw `u32` value to `OtaState`.
    pub fn from_u32(val: u32) -> Self {
        match val {
            0 => OtaState::New,
            1 => OtaState::PendingVerify,
            2 => OtaState::Valid,
            3 => OtaState::Invalid,
            4 => OtaState::Aborted,
            _ => OtaState::Undefined,
        }
    }
}

/// ESP-IDF `esp_ota_select_entry_t` structure (32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EspOtaSelectEntry {
    /// OTA sequence number.
    pub ota_seq: u32,
    /// Optional sequence label / padding.
    pub seq_label: [u8; 20],
    /// Operational state of the slot (`OtaState`).
    pub ota_state: u32,
    /// CRC-32 of `ota_seq`.
    pub crc: u32,
}

impl EspOtaSelectEntry {
    /// Fixed byte length of the entry in flash.
    pub const SIZE: usize = 32;

    /// Construct a new entry with computed CRC-32.
    pub fn new(ota_seq: u32, ota_state: OtaState) -> Self {
        let crc = compute_crc(ota_seq);
        Self {
            ota_seq,
            seq_label: [0u8; 20],
            ota_state: ota_state as u32,
            crc,
        }
    }

    /// Parse structure from a byte slice. Returns `None` if slice is too short.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        let ota_seq = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
        let mut seq_label = [0u8; 20];
        seq_label.copy_from_slice(&bytes[4..24]);
        let ota_state = u32::from_le_bytes(bytes[24..28].try_into().ok()?);
        let crc = u32::from_le_bytes(bytes[28..32].try_into().ok()?);

        Some(Self {
            ota_seq,
            seq_label,
            ota_state,
            crc,
        })
    }

    /// Serialize structure into little-endian 32-byte representation.
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0xffu8; Self::SIZE];
        bytes[0..4].copy_from_slice(&self.ota_seq.to_le_bytes());
        bytes[4..24].copy_from_slice(&self.seq_label);
        bytes[24..28].copy_from_slice(&self.ota_state.to_le_bytes());
        bytes[28..32].copy_from_slice(&self.crc.to_le_bytes());
        bytes
    }

    /// Check if the entry is valid for bootloader selection.
    pub fn is_valid(&self) -> bool {
        self.ota_seq != 0xFFFF_FFFF
            && self.ota_seq != 0
            && self.crc == compute_crc(self.ota_seq)
            && self.ota_state != OtaState::Invalid as u32
            && self.ota_state != OtaState::Aborted as u32
    }
}

/// Errors occurring during bootloader state operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootloaderError {
    /// Flash I/O or alignment failure.
    Flash,
    /// Invalid slot specification.
    InvalidSlot,
    /// Reserved, exhausted, or non-increasing OTA sequence number.
    InvalidSequence,
}

/// In-memory implementation of `OtaFlashStorage` for host testing.
#[derive(Debug, Clone)]
pub struct MemOtaFlash {
    /// Sector 0 entry buffer.
    pub sector0: [u8; OTA_DATA_ENTRY_SIZE],
    /// Sector 1 entry buffer.
    pub sector1: [u8; OTA_DATA_ENTRY_SIZE],
}

impl MemOtaFlash {
    /// Create new in-memory flash initialized with given entry buffers.
    pub fn new(sector0: [u8; OTA_DATA_ENTRY_SIZE], sector1: [u8; OTA_DATA_ENTRY_SIZE]) -> Self {
        Self { sector0, sector1 }
    }

    /// Create blank/unwritten in-memory flash (all 0xFF).
    pub fn blank() -> Self {
        Self {
            sector0: [0xffu8; OTA_DATA_ENTRY_SIZE],
            sector1: [0xffu8; OTA_DATA_ENTRY_SIZE],
        }
    }
}

impl OtaFlashStorage for MemOtaFlash {
    type Error = core::convert::Infallible;

    fn read_otadata(
        &mut self,
        sector0: &mut [u8; OTA_DATA_ENTRY_SIZE],
        sector1: &mut [u8; OTA_DATA_ENTRY_SIZE],
    ) -> Result<(), Self::Error> {
        sector0.copy_from_slice(&self.sector0);
        sector1.copy_from_slice(&self.sector1);
        Ok(())
    }

    fn write_otadata_sector(
        &mut self,
        sector_index: usize,
        data: &[u8; OTA_DATA_ENTRY_SIZE],
    ) -> Result<(), Self::Error> {
        match sector_index {
            0 => self.sector0.copy_from_slice(data),
            1 => self.sector1.copy_from_slice(data),
            _ => panic!("Invalid sector index"),
        }
        Ok(())
    }
}

// ============================================================================
// Constants
// ============================================================================

/// ESP32 SPI flash sector size (4 KiB).
pub const OTA_DATA_SECTOR_SIZE: usize = 4096;

/// ESP-IDF `esp_ota_select_entry_t` size at the start of each otadata sector (32 bytes).
pub const OTA_DATA_ENTRY_SIZE: usize = EspOtaSelectEntry::SIZE;

/// Absolute flash offset of `otadata` partition (from partitions.csv).
pub const OTA_DATA_OFFSET: u32 = 0x00f000;

/// Absolute flash offset of sector 0.
pub const OTA_DATA_SECTOR_0_OFFSET: u32 = OTA_DATA_OFFSET;

/// Absolute flash offset of sector 1.
pub const OTA_DATA_SECTOR_1_OFFSET: u32 = OTA_DATA_OFFSET + OTA_DATA_SECTOR_SIZE as u32;

// ============================================================================
// Target Hardware SPI Flash Implementation (RISC-V only)
// ============================================================================

#[cfg(target_arch = "riscv32")]
pub type DefaultFlash = EspOtaFlash;

#[cfg(not(target_arch = "riscv32"))]
pub type DefaultFlash = MemOtaFlash;

#[cfg(target_arch = "riscv32")]
pub struct EspOtaFlash;

#[cfg(target_arch = "riscv32")]
#[derive(Debug, Clone, Copy)]
pub enum EspOtaFlashError {
    Rom(i32),
    InvalidSector,
}

#[cfg(target_arch = "riscv32")]
impl OtaFlashStorage for EspOtaFlash {
    type Error = EspOtaFlashError;

    fn read_otadata(
        &mut self,
        sector0: &mut [u8; OTA_DATA_ENTRY_SIZE],
        sector1: &mut [u8; OTA_DATA_ENTRY_SIZE],
    ) -> Result<(), Self::Error> {
        let mut buf0 = [0u32; OTA_DATA_ENTRY_SIZE / 4];
        let ret0 = unsafe {
            esp_hal::rom::spiflash::esp_rom_spiflash_read(
                OTA_DATA_SECTOR_0_OFFSET,
                buf0.as_mut_ptr(),
                OTA_DATA_ENTRY_SIZE as u32,
            )
        };
        if ret0 != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            return Err(EspOtaFlashError::Rom(ret0));
        }
        unsafe {
            core::ptr::copy_nonoverlapping(
                buf0.as_ptr() as *const u8,
                sector0.as_mut_ptr(),
                OTA_DATA_ENTRY_SIZE,
            );
        }

        let mut buf1 = [0u32; OTA_DATA_ENTRY_SIZE / 4];
        let ret1 = unsafe {
            esp_hal::rom::spiflash::esp_rom_spiflash_read(
                OTA_DATA_SECTOR_1_OFFSET,
                buf1.as_mut_ptr(),
                OTA_DATA_ENTRY_SIZE as u32,
            )
        };
        if ret1 != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            return Err(EspOtaFlashError::Rom(ret1));
        }
        unsafe {
            core::ptr::copy_nonoverlapping(
                buf1.as_ptr() as *const u8,
                sector1.as_mut_ptr(),
                OTA_DATA_ENTRY_SIZE,
            );
        }

        Ok(())
    }

    fn write_otadata_sector(
        &mut self,
        sector_index: usize,
        data: &[u8; OTA_DATA_ENTRY_SIZE],
    ) -> Result<(), Self::Error> {
        let (offset, sector_num) = match sector_index {
            0 => (
                OTA_DATA_SECTOR_0_OFFSET,
                OTA_DATA_SECTOR_0_OFFSET / OTA_DATA_SECTOR_SIZE as u32,
            ),
            1 => (
                OTA_DATA_SECTOR_1_OFFSET,
                OTA_DATA_SECTOR_1_OFFSET / OTA_DATA_SECTOR_SIZE as u32,
            ),
            _ => return Err(EspOtaFlashError::InvalidSector),
        };

        let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_unlock() };
        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            warn!("Flash unlock failed: {}", ret);
            return Err(EspOtaFlashError::Rom(ret));
        }

        let ret = unsafe { esp_hal::rom::spiflash::esp_rom_spiflash_erase_sector(sector_num) };
        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            warn!("Flash erase sector {} failed: {}", sector_num, ret);
            return Err(EspOtaFlashError::Rom(ret));
        }

        let mut aligned = [0u32; OTA_DATA_ENTRY_SIZE / 4];
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                aligned.as_mut_ptr() as *mut u8,
                OTA_DATA_ENTRY_SIZE,
            );
        }

        let ret = unsafe {
            esp_hal::rom::spiflash::esp_rom_spiflash_write(
                offset,
                aligned.as_ptr(),
                OTA_DATA_ENTRY_SIZE as u32,
            )
        };
        if ret != esp_hal::rom::spiflash::ESP_ROM_SPIFLASH_RESULT_OK {
            warn!("Flash write at 0x{:x} failed: {}", offset, ret);
            return Err(EspOtaFlashError::Rom(ret));
        }

        Ok(())
    }
}

// ============================================================================
// Internal CRC-32 Lookup Table (IEEE 802.3 Little-Endian Polynomial 0xEDB88320)
// ============================================================================

static CRC32_LE_TABLE: [u32; 256] = [
    0x00000000, 0x77073096, 0xee0e612c, 0x990951ba, 0x076dc419, 0x706af48f, 0xe963a535, 0x9e6495a3,
    0x0edb8832, 0x79dcb8a4, 0xe0d5e91e, 0x97d2d988, 0x09b64c2b, 0x7eb17cbd, 0xe7b82d07, 0x90bf1d91,
    0x1db71064, 0x6ab020f2, 0xf3b97148, 0x84be41de, 0x1adad47d, 0x6ddde4eb, 0xf4d4b551, 0x83d385c7,
    0x136c9856, 0x646ba8c0, 0xfd62f97a, 0x8a65c9ec, 0x14015c4f, 0x63066cd9, 0xfa0f3d63, 0x8d080df5,
    0x3b6e20c8, 0x4c69105e, 0xd56041e4, 0xa2677172, 0x3c03e4d1, 0x4b04d447, 0xd20d85fd, 0xa50ab56b,
    0x35b5a8fa, 0x42b2986c, 0xdbbbc9d6, 0xacbcf940, 0x32d86ce3, 0x45df5c75, 0xdcd60dcf, 0xabd13d59,
    0x26d930ac, 0x51de003a, 0xc8d75180, 0xbfd06116, 0x21b4f4b5, 0x56b3c423, 0xcfba9599, 0xb8bda50f,
    0x2802b89e, 0x5f058808, 0xc60cd9b2, 0xb10be924, 0x2f6f7c87, 0x58684c11, 0xc1611dab, 0xb6662d3d,
    0x76dc4190, 0x01db7106, 0x98d220bc, 0xefd5102a, 0x71b18589, 0x06b6b51f, 0x9fbfe4a5, 0xe8b8d433,
    0x7807c9a2, 0x0f00f934, 0x9609a88e, 0xe10e9818, 0x7f6a0dbb, 0x086d3d2d, 0x91646c97, 0xe6635c01,
    0x6b6b51f4, 0x1c6c6162, 0x856530d8, 0xf262004e, 0x6c0695ed, 0x1b01a57b, 0x8208f4c1, 0xf50fc457,
    0x65b0d9c6, 0x12b7e950, 0x8bbeb8ea, 0xfcb9887c, 0x62dd1ddf, 0x15da2d49, 0x8cd37cf3, 0xfbd44c65,
    0x4db26158, 0x3ab551ce, 0xa3bc0074, 0xd4bb30e2, 0x4adfa541, 0x3dd895d7, 0xa4d1c46d, 0xd3d6f4fb,
    0x4369e96a, 0x346ed9fc, 0xad678846, 0xda60b8d0, 0x44042d73, 0x33031de5, 0xaa0a4c5f, 0xdd0d7cc9,
    0x5005713c, 0x270241aa, 0xbe0b1010, 0xc90c2086, 0x5768b525, 0x206f85b3, 0xb966d409, 0xce61e49f,
    0x5edef90e, 0x29d9c998, 0xb0d09822, 0xc7d7a8b4, 0x59b33d17, 0x2eb40d81, 0xb7bd5c3b, 0xc0ba6cad,
    0xedb88320, 0x9abfb3b6, 0x03b6e20c, 0x74b1d29a, 0xead54739, 0x9dd277af, 0x04db2615, 0x73dc1683,
    0xe3630b12, 0x94643b84, 0x0d6d6a3e, 0x7a6a5aa8, 0xe40ecf0b, 0x9309ff9d, 0x0a00ae27, 0x7d079eb1,
    0xf00f9344, 0x8708a3d2, 0x1e01f268, 0x6906c2fe, 0xf762575d, 0x806567cb, 0x196c3671, 0x6e6b06e7,
    0xfed41b76, 0x89d32be0, 0x10da7a5a, 0x67dd4acc, 0xf9b9df6f, 0x8ebeeff9, 0x17b7be43, 0x60b08ed5,
    0xd6d6a3e8, 0xa1d1937e, 0x38d8c2c4, 0x4fdff252, 0xd1bb67f1, 0xa6bc5767, 0x3fb506dd, 0x48b2364b,
    0xd80d2bda, 0xaf0a1b4c, 0x36034af6, 0x41047a60, 0xdf60efc3, 0xa867df55, 0x316e8eef, 0x4669be79,
    0xcb61b38c, 0xbc66831a, 0x256fd2a0, 0x5268e236, 0xcc0c7795, 0xbb0b4703, 0x220216b9, 0x5505262f,
    0xc5ba3bbe, 0xb2bd0b28, 0x2bb45a92, 0x5cb36a04, 0xc2d7ffa7, 0xb5d0cf31, 0x2cd99e8b, 0x5bdeae1d,
    0x9b64c2b0, 0xec63f226, 0x756aa39c, 0x026d930a, 0x9c0906a9, 0xeb0e363f, 0x72076785, 0x05005713,
    0x95bf4a82, 0xe2b87a14, 0x7bb12bae, 0x0cb61b38, 0x92d28e9b, 0xe5d5be0d, 0x7cdcefb7, 0x0bdbdf21,
    0x86d3d2d4, 0xf1d4e242, 0x68ddb3f8, 0x1fda836e, 0x81be16cd, 0xf6b9265b, 0x6fb077e1, 0x18b74777,
    0x88085ae6, 0xff0f6a70, 0x66063bca, 0x11010b5c, 0x8f659eff, 0xf862ae69, 0x616bffd3, 0x166ccf45,
    0xa00ae278, 0xd70dd2ee, 0x4e048354, 0x3903b3c2, 0xa7672661, 0xd06016f7, 0x4969474d, 0x3e6e77db,
    0xaed16a4a, 0xd9d65adc, 0x40df0b66, 0x37d83bf0, 0xa9bcae53, 0xdebb9ec5, 0x47b2cf7f, 0x30b5ffe9,
    0xbdbdf21c, 0xcabac28a, 0x53b39330, 0x24b4a3a6, 0xbad03605, 0xcdd70693, 0x54de5729, 0x23d967bf,
    0xb3667a2e, 0xc4614ab8, 0x5d681b02, 0x2a6f2b94, 0xb40bbe37, 0xc30c8ea1, 0x5a05df1b, 0x2d02ef8d,
];

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_slot_with_higher_valid_sequence() {
        // Arrange
        let mut sector0 = [0xffu8; OTA_DATA_ENTRY_SIZE];
        let mut sector1 = [0xffu8; OTA_DATA_ENTRY_SIZE];
        write_ota_entry(&mut sector0, 1, OtaState::Valid);
        write_ota_entry(&mut sector1, 2, OtaState::Valid);

        // Act
        let active = resolve_active_slot(&sector0, &sector1);

        // Assert
        assert_eq!(active.seq, 2);
        assert_eq!(active.slot, Slot::Ota1);
        assert_eq!(active.sector_index, 1);
    }

    #[test]
    fn selects_slot_with_higher_valid_sequence_reversed() {
        // Arrange
        let mut sector0 = [0xffu8; OTA_DATA_ENTRY_SIZE];
        let mut sector1 = [0xffu8; OTA_DATA_ENTRY_SIZE];
        write_ota_entry(&mut sector0, 3, OtaState::Valid);
        write_ota_entry(&mut sector1, 2, OtaState::Valid);

        // Act
        let active = resolve_active_slot(&sector0, &sector1);

        // Assert
        assert_eq!(active.seq, 3);
        assert_eq!(active.slot, Slot::Ota0);
        assert_eq!(active.sector_index, 0);
    }

    #[test]
    fn ignores_corrupted_crc() {
        // Arrange
        let mut sector0 = [0xffu8; OTA_DATA_ENTRY_SIZE];
        let mut sector1 = [0xffu8; OTA_DATA_ENTRY_SIZE];
        write_ota_entry(&mut sector0, 5, OtaState::Valid);
        // Corrupt CRC in sector 0
        sector0[30] ^= 0xff;
        write_ota_entry(&mut sector1, 2, OtaState::Valid);

        // Act
        let active = resolve_active_slot(&sector0, &sector1);

        // Assert
        assert_eq!(active.seq, 2);
        assert_eq!(active.slot, Slot::Ota1);
        assert_eq!(active.sector_index, 1);
    }

    #[test]
    fn defaults_to_ota0_when_flash_is_blank() {
        // Arrange
        let sector0 = [0xffu8; OTA_DATA_ENTRY_SIZE];
        let sector1 = [0xffu8; OTA_DATA_ENTRY_SIZE];

        // Act
        let active = resolve_active_slot(&sector0, &sector1);

        // Assert
        assert_eq!(active.seq, 0);
        assert_eq!(active.slot, Slot::Ota0);
        assert_eq!(active.state, OtaState::Undefined);
        assert_eq!(active.sector_index, 0);
    }

    #[test]
    fn ignores_invalid_and_aborted_states() {
        // Arrange
        let mut sector0 = [0xffu8; OTA_DATA_ENTRY_SIZE];
        let mut sector1 = [0xffu8; OTA_DATA_ENTRY_SIZE];
        write_ota_entry(&mut sector0, 1, OtaState::Valid);
        write_ota_entry(&mut sector1, 2, OtaState::Invalid);

        // Act
        let active = resolve_active_slot(&sector0, &sector1);

        // Assert
        assert_eq!(active.seq, 1);
        assert_eq!(active.slot, Slot::Ota0);
        assert_eq!(active.sector_index, 0);
    }

    #[test]
    fn slot_mapping_and_next_sequence_calculation() {
        // Arrange
        let seqs = [0, 1, 2, 3, 4];
        let slots = [Slot::Ota0, Slot::Ota1];

        // Act
        let mapped: alloc::vec::Vec<Slot> = seqs.iter().map(|&s| Slot::from_seq(s)).collect();
        let toggled: alloc::vec::Vec<Slot> = slots.iter().map(|&s| compute_target_slot(s)).collect();
        let next_ota1_from_0 = compute_next_seq(0, Slot::Ota1);
        let next_ota0_from_0 = compute_next_seq(0, Slot::Ota0);
        let next_ota1_from_1 = compute_next_seq(1, Slot::Ota1);
        let next_ota0_from_2 = compute_next_seq(2, Slot::Ota0);
        let next_ota1_from_3 = compute_next_seq(3, Slot::Ota1);

        // Assert
        assert_eq!(mapped, [Slot::Ota0, Slot::Ota0, Slot::Ota1, Slot::Ota0, Slot::Ota1]);
        assert_eq!(toggled, [Slot::Ota1, Slot::Ota0]);
        assert_eq!(Slot::Ota0.other(), Slot::Ota1);
        assert_eq!(Slot::Ota1.other(), Slot::Ota0);

        assert_eq!(next_ota1_from_0, 2);
        assert_eq!(next_ota0_from_0, 1);
        assert_eq!(next_ota1_from_1, 2);
        assert_eq!(next_ota0_from_2, 3);
        assert_eq!(next_ota1_from_3, 4);
    }

    #[test]
    fn compute_next_seq_handles_u32_max_without_looping() {
        // Arrange
        let max_seq = u32::MAX;
        let near_max = u32::MAX - 1;

        // Act
        let next_for_ota1_from_max = compute_next_seq(max_seq, Slot::Ota1);
        let next_for_ota0_from_max = compute_next_seq(max_seq, Slot::Ota0);
        let next_for_ota0_from_near_max = compute_next_seq(near_max, Slot::Ota0);
        let next_for_ota1_from_near_max = compute_next_seq(near_max, Slot::Ota1);

        // Assert
        assert_eq!(next_for_ota1_from_max, 2);
        assert_eq!(next_for_ota0_from_max, 1);
        assert_eq!(next_for_ota0_from_near_max, u32::MAX);
        assert_eq!(next_for_ota1_from_near_max, 2);
    }

    #[test]
    fn entry_serialization_and_crc_verification() {
        // Arrange
        let entry = EspOtaSelectEntry::new(42, OtaState::PendingVerify);

        // Act
        let bytes = entry.to_bytes();
        let parsed = EspOtaSelectEntry::from_bytes(&bytes).unwrap();

        // Assert
        assert_eq!(parsed.ota_seq, 42);
        assert_eq!(parsed.ota_state, OtaState::PendingVerify as u32);
        assert_eq!(parsed.crc, compute_crc(42));
        assert_eq!(parsed.crc, 0x3070b0a5);
        assert_eq!(compute_crc(1), 0x4743989a);
        assert_eq!(compute_crc(2), 0x55f63774);
        assert_eq!(compute_crc(3), 0xed4a5011);
        assert!(parsed.is_valid());
    }

    #[test]
    fn bootloader_state_transitions_with_in_memory_flash() {
        // Arrange
        let flash = MemOtaFlash::blank();
        let mut bootloader = OtaBootloader::new(flash).unwrap();

        // Act 1: Initial state on blank flash
        assert_eq!(bootloader.get_running_slot(), Slot::Ota0);
        assert_eq!(bootloader.get_running_seq(), 0);
        assert_eq!(bootloader.get_target_slot(), Slot::Ota1);
        assert_eq!(bootloader.get_next_seq(), 2);

        // Act 2: Flash update into target slot (Ota1, seq 2)
        let target = bootloader.get_target_slot();
        let seq = bootloader.get_next_seq();
        bootloader.mark_pending_verify(target, seq).unwrap();

        // Assert 2
        assert_eq!(bootloader.get_running_slot(), Slot::Ota1);
        assert_eq!(bootloader.get_running_seq(), 2);
        assert_eq!(bootloader.get_target_slot(), Slot::Ota0);
        assert_eq!(bootloader.get_next_seq(), 3);

        // Act 3: Confirm valid on boot
        bootloader.mark_valid().unwrap();

        // Assert 3
        assert_eq!(bootloader.get_active().state, OtaState::Valid);
        assert_eq!(bootloader.get_running_slot(), Slot::Ota1);
    }

    #[test]
    fn mark_valid_is_idempotent() {
        // Arrange
        let mut sector0 = [0xffu8; OTA_DATA_ENTRY_SIZE];
        let sector1 = [0xffu8; OTA_DATA_ENTRY_SIZE];
        write_ota_entry(&mut sector0, 1, OtaState::Valid);
        let flash = MemOtaFlash::new(sector0, sector1);
        let mut bootloader = OtaBootloader::new(flash).unwrap();

        // Act
        let res = bootloader.mark_valid();

        // Assert
        assert!(res.is_ok());
        assert_eq!(bootloader.get_active().state, OtaState::Valid);
    }

    use std::cell::RefCell;
    use std::convert::Infallible;
    use std::rc::Rc;

    #[derive(Clone)]
    struct RecordingFlash {
        state: Rc<RefCell<FlashState>>,
    }

    #[derive(Clone)]
    struct FlashState {
        sectors: [[u8; OTA_DATA_ENTRY_SIZE]; 2],
        writes: usize,
    }

    impl OtaFlashStorage for RecordingFlash {
        type Error = Infallible;

        fn read_otadata(
            &mut self,
            sector0: &mut [u8; OTA_DATA_ENTRY_SIZE],
            sector1: &mut [u8; OTA_DATA_ENTRY_SIZE],
        ) -> Result<(), Self::Error> {
            let state = self.state.borrow();
            sector0.copy_from_slice(&state.sectors[0]);
            sector1.copy_from_slice(&state.sectors[1]);
            Ok(())
        }

        fn write_otadata_sector(
            &mut self,
            sector_index: usize,
            data: &[u8; OTA_DATA_ENTRY_SIZE],
        ) -> Result<(), Self::Error> {
            let mut state = self.state.borrow_mut();
            state.writes += 1;
            state.sectors[sector_index].copy_from_slice(data);
            Ok(())
        }
    }

    fn with_active_seq(seq: u32) -> (OtaBootloader<RecordingFlash>, RecordingFlash) {
        let mut sectors = [[0xff; OTA_DATA_ENTRY_SIZE]; 2];
        if seq != 0 {
            write_ota_entry(&mut sectors[0], seq, OtaState::Valid);
        }
        let flash = RecordingFlash {
            state: Rc::new(RefCell::new(FlashState { sectors, writes: 0 })),
        };
        let bootloader = OtaBootloader::new(flash.clone()).unwrap();
        (bootloader, flash)
    }

    fn assert_rejected_without_write(
        active_seq: u32,
        target: Slot,
        candidate: u32,
        expected: BootloaderError,
    ) {
        let (mut bootloader, flash) = with_active_seq(active_seq);
        let active_before = bootloader.get_active();
        let sectors_before = flash.state.borrow().sectors;

        let result = bootloader.mark_pending_verify(target, candidate);

        assert_eq!(result, Err(expected), "candidate {candidate:#010x}");
        assert_eq!(bootloader.get_active(), active_before);
        let state = flash.state.borrow();
        assert_eq!(state.writes, 0);
        assert_eq!(state.sectors, sectors_before);
    }

    #[test]
    fn reserved_max_sequence_from_fffffffe_is_rejected_without_write() {
        let (bootloader, _) = with_active_seq(0xffff_fffe);
        assert_eq!(bootloader.get_next_seq(), u32::MAX);
        assert_rejected_without_write(
            0xffff_fffe,
            Slot::Ota0,
            u32::MAX,
            BootloaderError::InvalidSequence,
        );
    }

    #[test]
    fn wrapped_lower_sequence_from_fffffffe_is_rejected_without_write() {
        assert_rejected_without_write(0xffff_fffe, Slot::Ota0, 1, BootloaderError::InvalidSequence);
    }

    #[test]
    fn non_increasing_sequence_from_fffffffd_is_rejected_without_write() {
        assert_rejected_without_write(0xffff_fffd, Slot::Ota1, 2, BootloaderError::InvalidSequence);
    }

    #[test]
    fn zero_sequence_is_rejected_without_write() {
        assert_rejected_without_write(0, Slot::Ota1, 0, BootloaderError::InvalidSequence);
    }

    #[test]
    fn sequence_mapping_to_wrong_slot_is_rejected_without_write() {
        assert_rejected_without_write(1, Slot::Ota1, 3, BootloaderError::InvalidSlot);
    }

    #[test]
    fn active_slot_is_not_accepted_as_target() {
        assert_rejected_without_write(1, Slot::Ota0, 3, BootloaderError::InvalidSlot);
    }

    #[test]
    fn highest_usable_sequence_survives_reboot_selection() {
        let (mut bootloader, flash) = with_active_seq(0xffff_fffd);
        let next_seq = bootloader.get_next_seq();
        assert_eq!(next_seq, 0xffff_fffe);

        assert_eq!(bootloader.mark_pending_verify(Slot::Ota1, next_seq), Ok(()));
        assert_eq!(flash.state.borrow().writes, 1);

        let reloaded = OtaBootloader::new(flash).unwrap();
        assert_eq!(reloaded.get_active().seq, next_seq);
        assert_eq!(reloaded.get_active().slot, Slot::Ota1);
        assert_eq!(reloaded.get_active().state, OtaState::New);
    }

    #[test]
    fn failed_sequence_reports_an_error_before_any_flash_write() {
        let (mut bootloader, flash) = with_active_seq(0xffff_fffe);
        let result: Result<(), BootloaderError> =
            bootloader.mark_pending_verify(bootloader.get_target_slot(), bootloader.get_next_seq());
        assert_eq!(result, Err(BootloaderError::InvalidSequence));
        assert_eq!(flash.state.borrow().writes, 0);
    }

}
