#![no_std]

pub mod handler;
pub mod route;
pub mod stream;

pub use handler::{
    complete_ota_response, handle_ota_flash, OtaPolicy, OtaResponse, MAX_CONTENT_LENGTH,
    MIN_CONTENT_LENGTH,
};
pub use route::{parse_ota_route, OtaRoute};
pub use stream::PrefixedStream;
