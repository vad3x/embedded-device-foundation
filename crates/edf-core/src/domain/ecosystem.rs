//! Smart home ecosystem domain model.
//!
//! Protocol-agnostic value objects and status representations for smart home ecosystems.

// ============================================================================
// Public Constants & Types
// ============================================================================

/// Maximum number of free-form key-value telemetry properties per ecosystem.
pub const MAX_ECOSYSTEM_PROPERTIES: usize = 6;

/// Strongly-typed value object identifying a smart home ecosystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EcosystemId(&'static str);

impl EcosystemId {
    /// Create a new `EcosystemId` from a static string identifier.
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// Return the underlying string slice identifier.
    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

/// Trait implemented by composition-root ecosystem enums to map variants to [`EcosystemId`].
pub trait EcosystemType: Copy + Eq + core::fmt::Debug + 'static {
    /// Return the strongly-typed [`EcosystemId`] for this ecosystem variant.
    fn id(&self) -> EcosystemId;
}

impl EcosystemType for EcosystemId {
    fn id(&self) -> EcosystemId {
        *self
    }
}

/// Typed, zero-allocation value for per-ecosystem telemetry properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcosystemValue {
    Bool(bool),
    Int(i64),
    Text(&'static str),
}

/// Maximum number of active smart home fabrics/hubs tracked in telemetry.
pub const MAX_ECOSYSTEM_FABRICS: usize = 5;

/// Fixed-capacity 32-byte string buffer for fabric labels, implementing Copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FabricLabel {
    bytes: [u8; 32],
    len: u8,
}

impl FabricLabel {
    /// Create an empty fabric label.
    pub const fn empty() -> Self {
        Self {
            bytes: [0; 32],
            len: 0,
        }
    }

    /// Create a fabric label from a string slice, truncating to 32 bytes if necessary.
    #[expect(
        clippy::should_implement_trait,
        reason = "preserve the existing infallible FabricLabel constructor API"
    )]
    pub fn from_str(s: &str) -> Self {
        let mut bytes = [0u8; 32];
        let copy_len = core::cmp::min(s.len(), 32);
        bytes[..copy_len].copy_from_slice(&s.as_bytes()[..copy_len]);
        Self {
            bytes,
            len: copy_len as u8,
        }
    }

    /// Return the label as a string slice.
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

/// Maximum number of subscriptions tracked per fabric.
pub const MAX_FABRIC_SUBSCRIPTIONS: usize = 4;

/// Subscription descriptor identifying an active interaction model subscription on a fabric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FabricSubscription {
    /// Numerical endpoint ID (e.g. 1, 2)
    pub endpoint_id: u16,
    /// Numerical cluster ID (e.g. 0x003B, 0x000A)
    pub cluster_id: u32,
}

impl FabricSubscription {
    pub const fn new(endpoint_id: u16, cluster_id: u32) -> Self {
        Self {
            endpoint_id,
            cluster_id,
        }
    }
}

/// Telemetry snapshot for an individual connected smart home fabric / hub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FabricInfo {
    /// 1-based fabric index within the device's fabric table (1..=254)
    pub fab_idx: u8,
    /// Vendor ID of the commissioner or ecosystem (e.g. 0x1349, 0x1384, 0x130D, 0x134B)
    pub vendor_id: u16,
    /// 64-bit Fabric ID assigned by the operational CA
    pub fabric_id: u64,
    /// 64-bit Node ID assigned to this accessory on the fabric
    pub node_id: u64,
    /// Human-readable label configured by user or ecosystem (up to 32 bytes)
    pub label: FabricLabel,
    /// Active subscriptions established by this fabric
    pub subscriptions: [Option<FabricSubscription>; MAX_FABRIC_SUBSCRIPTIONS],
}

impl FabricInfo {
    /// Create a new `FabricInfo` with no subscriptions.
    pub const fn new(
        fab_idx: u8,
        vendor_id: u16,
        fabric_id: u64,
        node_id: u64,
        label: FabricLabel,
    ) -> Self {
        Self {
            fab_idx,
            vendor_id,
            fabric_id,
            node_id,
            label,
            subscriptions: [None; MAX_FABRIC_SUBSCRIPTIONS],
        }
    }

    /// Add an active subscription to this fabric.
    pub fn with_subscription(mut self, sub: FabricSubscription) -> Self {
        for slot in self.subscriptions.iter_mut() {
            if slot.is_none() {
                *slot = Some(sub);
                break;
            }
        }
        self
    }

    /// Iterate over active subscriptions established by this fabric.
    pub fn iter_subscriptions(&self) -> impl Iterator<Item = &FabricSubscription> {
        self.subscriptions.iter().filter_map(|slot| slot.as_ref())
    }
}

/// Maximum number of exposed smart home endpoints.
pub const MAX_ECOSYSTEM_ENDPOINTS: usize = 4;

/// Maximum number of clusters tracked per endpoint.
pub const MAX_CLUSTERS_PER_ENDPOINT: usize = 6;

/// Snapshot descriptor for a cluster exposed by an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClusterInfo {
    /// Numerical cluster ID (e.g. 0x003B, 0x0551, 0x000A)
    pub id: u32,
    /// Standard cluster name (e.g. "Switch", "Camera AV Stream Management")
    pub name: &'static str,
}

impl ClusterInfo {
    pub const fn new(id: u32, name: &'static str) -> Self {
        Self { id, name }
    }
}

/// Snapshot descriptor for an exposed smart home endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointInfo {
    /// Numerical endpoint ID (e.g. 1, 2)
    pub id: u16,
    /// Human-readable name (e.g. "Audio Doorbell", "Door Lock")
    pub name: &'static str,
    /// Matter device type ID (e.g. 0x0141, 0x000A)
    pub device_type: u32,
    /// Clusters hosted on this endpoint
    pub clusters: [Option<ClusterInfo>; MAX_CLUSTERS_PER_ENDPOINT],
}

impl EndpointInfo {
    /// Create a new `EndpointInfo` with no clusters.
    pub const fn new(id: u16, name: &'static str, device_type: u32) -> Self {
        Self {
            id,
            name,
            device_type,
            clusters: [None; MAX_CLUSTERS_PER_ENDPOINT],
        }
    }

    /// Add a cluster to this endpoint.
    pub fn with_cluster(mut self, cluster: ClusterInfo) -> Self {
        for slot in self.clusters.iter_mut() {
            if slot.is_none() {
                *slot = Some(cluster);
                break;
            }
        }
        self
    }

    /// Iterate over all clusters on this endpoint.
    pub fn iter_clusters(&self) -> impl Iterator<Item = &ClusterInfo> {
        self.clusters.iter().filter_map(|slot| slot.as_ref())
    }
}

/// Status and telemetry snapshot of a single active smart home ecosystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EcosystemStatus {
    pub ecosystem_type: EcosystemId,
    pub paired: bool,
    pub properties: [Option<(&'static str, EcosystemValue)>; MAX_ECOSYSTEM_PROPERTIES],
    pub fabrics: [Option<FabricInfo>; MAX_ECOSYSTEM_FABRICS],
    pub endpoints: [Option<EndpointInfo>; MAX_ECOSYSTEM_ENDPOINTS],
}

impl EcosystemStatus {
    /// Create a new `EcosystemStatus` with no extra properties, fabrics, or endpoints.
    pub const fn new(ecosystem_type: EcosystemId, paired: bool) -> Self {
        Self {
            ecosystem_type,
            paired,
            properties: [None; MAX_ECOSYSTEM_PROPERTIES],
            fabrics: [None; MAX_ECOSYSTEM_FABRICS],
            endpoints: [None; MAX_ECOSYSTEM_ENDPOINTS],
        }
    }

    /// Add a connected fabric / hub to the telemetry snapshot.
    pub fn with_fabric(mut self, fabric: FabricInfo) -> Self {
        for slot in self.fabrics.iter_mut() {
            if slot.is_none() {
                *slot = Some(fabric);
                break;
            }
        }
        self
    }

    /// Add an exposed endpoint descriptor to the telemetry snapshot.
    pub fn with_endpoint(mut self, endpoint: EndpointInfo) -> Self {
        for slot in self.endpoints.iter_mut() {
            if slot.is_none() {
                *slot = Some(endpoint);
                break;
            }
        }
        self
    }

    /// Iterate over all active connected fabrics in insertion order.
    pub fn iter_fabrics(&self) -> impl Iterator<Item = &FabricInfo> {
        self.fabrics.iter().filter_map(|slot| slot.as_ref())
    }

    /// Iterate over all exposed endpoints in insertion order.
    pub fn iter_endpoints(&self) -> impl Iterator<Item = &EndpointInfo> {
        self.endpoints.iter().filter_map(|slot| slot.as_ref())
    }

    /// Builder method to set or update a key-value property.
    ///
    /// If `key` already exists in `self.properties`, its value is updated;
    /// otherwise it is inserted into the first empty slot.
    pub const fn with_property(mut self, key: &'static str, value: EcosystemValue) -> Self {
        let mut i = 0;
        let mut first_none = None;
        while i < MAX_ECOSYSTEM_PROPERTIES {
            if let Some((k, _)) = self.properties[i] {
                if str_eq(k, key) {
                    self.properties[i] = Some((key, value));
                    return self;
                }
            } else if first_none.is_none() {
                first_none = Some(i);
            }
            i += 1;
        }
        if let Some(slot) = first_none {
            self.properties[slot] = Some((key, value));
        }
        self
    }

    /// Return the value of the property with the given key, if present.
    pub fn property(&self, key: &str) -> Option<&EcosystemValue> {
        self.properties.iter().find_map(|prop| match prop {
            Some((k, v)) if *k == key => Some(v),
            _ => None,
        })
    }

    /// Iterate over all populated `(key, value)` properties in insertion order.
    pub fn iter_properties(&self) -> impl Iterator<Item = (&'static str, EcosystemValue)> + '_ {
        self.properties.iter().filter_map(|prop| *prop)
    }
}

// ============================================================================
// Private Helper Functions
// ============================================================================

const fn str_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecosystem_id_as_str_returns_identifier() {
        // Arrange
        let id = EcosystemId::new("matter");

        // Act
        let s = id.as_str();

        // Assert
        assert_eq!(s, "matter");
        assert_eq!(id.id(), id);
    }

    #[test]
    fn ecosystem_status_property_lookup_and_overwrite() {
        // Arrange
        let id = EcosystemId::new("matter");
        let status = EcosystemStatus::new(id, true)
            .with_property("fabrics", EcosystemValue::Int(2))
            .with_property("subscriptions", EcosystemValue::Int(1));

        // Act
        let fabrics = status.property("fabrics");
        let subs = status.property("subscriptions");
        let non_existent = status.property("missing");

        // Assert
        assert_eq!(fabrics, Some(&EcosystemValue::Int(2)));
        assert_eq!(subs, Some(&EcosystemValue::Int(1)));
        assert_eq!(non_existent, None);

        // Act - overwrite existing key
        let updated = status.with_property("fabrics", EcosystemValue::Int(3));

        // Assert
        assert_eq!(updated.property("fabrics"), Some(&EcosystemValue::Int(3)));
    }

    #[test]
    fn ecosystem_status_iterates_populated_properties() {
        // Arrange
        let id = EcosystemId::new("matter");
        let status = EcosystemStatus::new(id, true)
            .with_property("online", EcosystemValue::Bool(true))
            .with_property("node_id", EcosystemValue::Text("0x1234"));

        // Act
        let props: Vec<(&'static str, EcosystemValue)> = status.iter_properties().collect();

        // Assert
        assert_eq!(props.len(), 2);
        assert_eq!(props[0], ("online", EcosystemValue::Bool(true)));
        assert_eq!(props[1], ("node_id", EcosystemValue::Text("0x1234")));
    }

    #[test]
    fn ecosystem_status_fabric_addition_and_iteration() {
        // Arrange
        let fab1 = FabricInfo::new(
            1,
            4996,
            0x1122334455667788,
            0x0000000000000002,
            FabricLabel::from_str("Apple HomePod"),
        )
        .with_subscription(FabricSubscription::new(1, 0x003B))
        .with_subscription(FabricSubscription::new(2, 0x000A));

        let fab2 = FabricInfo::new(
            2,
            4939,
            0xAABBCCDDEEFF0011,
            0x0000000000000005,
            FabricLabel::from_str("Home Assistant"),
        )
        .with_subscription(FabricSubscription::new(2, 0x000A));

        // Act
        let status = EcosystemStatus::new(EcosystemId::new("Matter"), true)
            .with_fabric(fab1)
            .with_fabric(fab2);

        let fabrics: Vec<_> = status.iter_fabrics().copied().collect();

        // Assert
        assert_eq!(fabrics.len(), 2);
        assert_eq!(fabrics[0], fab1);
        assert_eq!(fabrics[1], fab2);
        assert_eq!(fabrics[0].label.as_str(), "Apple HomePod");
        assert_eq!(fabrics[1].label.as_str(), "Home Assistant");
        let fab1_subs: Vec<_> = fabrics[0].iter_subscriptions().copied().collect();
        assert_eq!(fab1_subs.len(), 2);
        assert_eq!(fab1_subs[0], FabricSubscription::new(1, 0x003B));
        assert_eq!(fab1_subs[1], FabricSubscription::new(2, 0x000A));
        let fab2_subs: Vec<_> = fabrics[1].iter_subscriptions().copied().collect();
        assert_eq!(fab2_subs.len(), 1);
        assert_eq!(fab2_subs[0], FabricSubscription::new(2, 0x000A));
    }

    #[test]
    fn ecosystem_status_endpoint_addition_and_iteration() {
        // Arrange
        let ep1 = EndpointInfo::new(1, "Audio Doorbell", 0x0141)
            .with_cluster(ClusterInfo::new(0x003B, "Switch"))
            .with_cluster(ClusterInfo::new(0x0551, "Camera AV Stream Management"));
        let ep2 = EndpointInfo::new(2, "Door Lock", 0x000A)
            .with_cluster(ClusterInfo::new(0x000A, "Door Lock"));

        // Act
        let status = EcosystemStatus::new(EcosystemId::new("Matter"), true)
            .with_endpoint(ep1)
            .with_endpoint(ep2);

        let endpoints: Vec<_> = status.iter_endpoints().copied().collect();

        // Assert
        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0], ep1);
        assert_eq!(endpoints[1], ep2);
        let ep1_clusters: Vec<_> = ep1.iter_clusters().copied().collect();
        assert_eq!(ep1_clusters.len(), 2);
        assert_eq!(ep1_clusters[0], ClusterInfo::new(0x003B, "Switch"));
    }

    #[test]
    fn ecosystem_status_fabric_capacity_limit() {
        // Arrange
        let mut status = EcosystemStatus::new(EcosystemId::new("Matter"), true);

        // Act - add MAX_ECOSYSTEM_FABRICS + 2
        for i in 1..=7 {
            status = status.with_fabric(FabricInfo::new(
                i as u8,
                0x1000 + i,
                i as u64,
                i as u64,
                FabricLabel::from_str("Hub"),
            ));
        }

        // Assert - only MAX_ECOSYSTEM_FABRICS (5) retained without crashing
        assert_eq!(status.iter_fabrics().count(), MAX_ECOSYSTEM_FABRICS);
    }

    #[test]
    fn ecosystem_status_endpoint_capacity_limit() {
        // Arrange
        let mut status = EcosystemStatus::new(EcosystemId::new("Matter"), true);

        // Act - add MAX_ECOSYSTEM_ENDPOINTS + 2
        for i in 1..=MAX_ECOSYSTEM_ENDPOINTS + 2 {
            status = status.with_endpoint(EndpointInfo::new(i as u16, "Cluster", 0x0141));
        }

        // Assert - only MAX_ECOSYSTEM_ENDPOINTS (4) retained without crashing
        assert_eq!(status.iter_endpoints().count(), MAX_ECOSYSTEM_ENDPOINTS);
    }
}
