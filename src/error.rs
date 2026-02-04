//! Error types for ESP32 EMAC driver
//!
//! Errors are organized by domain for better diagnostics:
//! - [`ConfigError`]: Initialization and configuration failures
//! - [`DmaError`]: DMA buffer and descriptor issues
//! - [`IoError`]: Runtime TX/RX failures
//!
//! The unified [`Error`] enum wraps all domain errors and is returned
//! by most driver methods.

// =============================================================================
// Configuration Errors
// =============================================================================

/// Configuration and initialization errors
///
/// These errors occur during driver setup, clock configuration,
/// or PHY/GPIO initialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ConfigError {
    /// Driver already initialized
    AlreadyInitialized,
    /// Invalid configuration parameter
    InvalidConfig,
    /// Invalid PHY address (must be 0-31)
    InvalidPhyAddress,
    /// Clock configuration error
    ClockError,
    /// GPIO configuration error
    GpioError,
    /// Software reset failed or timed out
    ResetFailed,
}

impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl ConfigError {
    /// Returns a human-readable description of the error
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            ConfigError::AlreadyInitialized => "already initialized",
            ConfigError::InvalidConfig => "invalid configuration",
            ConfigError::InvalidPhyAddress => "invalid PHY address",
            ConfigError::ClockError => "clock configuration error",
            ConfigError::GpioError => "GPIO configuration error",
            ConfigError::ResetFailed => "software reset failed",
        }
    }
}

// =============================================================================
// DMA Errors
// =============================================================================

/// DMA buffer and descriptor errors
///
/// These errors relate to descriptor ring management and buffer allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DmaError {
    /// No descriptors available for transmission
    NoDescriptorsAvailable,
    /// Descriptor is busy (owned by DMA hardware)
    DescriptorBusy,
    /// Frame too large for buffer capacity
    FrameTooLarge,
    /// Invalid frame length (zero or exceeds maximum)
    InvalidLength,
    /// Fatal bus error (unrecoverable DMA error)
    FatalBusError,
}

impl core::fmt::Display for DmaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl DmaError {
    /// Returns a human-readable description of the error
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            DmaError::NoDescriptorsAvailable => "no descriptors available",
            DmaError::DescriptorBusy => "descriptor busy",
            DmaError::FrameTooLarge => "frame too large for buffers",
            DmaError::InvalidLength => "invalid frame length",
            DmaError::FatalBusError => "fatal DMA bus error",
        }
    }
}

// =============================================================================
// I/O Errors
// =============================================================================

/// Runtime TX/RX errors
///
/// These errors occur during frame transmission or reception.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum IoError {
    /// Operation timed out
    Timeout,
    /// Invalid state for operation (e.g., not running)
    InvalidState,
    /// Buffer too small for received frame
    BufferTooSmall,
    /// Incomplete frame received (missing first/last segment)
    IncompleteFrame,
    /// Frame has receive errors (CRC, overflow, etc.)
    FrameError,
    /// PHY communication error (MDIO timeout or failure)
    PhyError,
}

impl core::fmt::Display for IoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl IoError {
    /// Returns a human-readable description of the error
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            IoError::Timeout => "operation timed out",
            IoError::InvalidState => "invalid state for operation",
            IoError::BufferTooSmall => "buffer too small for frame",
            IoError::IncompleteFrame => "incomplete frame",
            IoError::FrameError => "frame error",
            IoError::PhyError => "PHY communication error",
        }
    }
}

// =============================================================================
// Unified Error Type
// =============================================================================

/// This enum wraps all domain-specific errors for unified error handling.
///
/// Match on the inner domain error for specific handling:
/// ```ignore
/// match result {
///     Err(Error::Config(ConfigError::InvalidPhyAddress)) => { /* ... */ }
///     Err(Error::Dma(DmaError::NoDescriptorsAvailable)) => { /* ... */ }
///     Err(Error::Io(IoError::Timeout)) => { /* ... */ }
///     _ => {}
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// Configuration error
    Config(ConfigError),
    /// DMA error
    Dma(DmaError),
    /// I/O error
    Io(IoError),
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Config(e) => write!(f, "config: {}", e.as_str()),
            Error::Dma(e) => write!(f, "dma: {}", e.as_str()),
            Error::Io(e) => write!(f, "io: {}", e.as_str()),
        }
    }
}

// From impls for automatic conversion
impl From<ConfigError> for Error {
    fn from(e: ConfigError) -> Self {
        Error::Config(e)
    }
}

impl From<DmaError> for Error {
    fn from(e: DmaError) -> Self {
        Error::Dma(e)
    }
}

impl From<IoError> for Error {
    fn from(e: IoError) -> Self {
        Error::Io(e)
    }
}

/// Result type alias for EMAC operations
pub type Result<T> = core::result::Result<T, Error>;

/// Result type alias for configuration operations
pub type ConfigResult<T> = core::result::Result<T, ConfigError>;

/// Result type alias for DMA operations
pub type DmaResult<T> = core::result::Result<T, DmaError>;

/// Result type alias for I/O operations
pub type IoResult<T> = core::result::Result<T, IoError>;

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    extern crate std;
    use std::format;

    use super::*;

    // =========================================================================
    // ConfigError Tests
    // =========================================================================

    #[test]
    fn config_error_as_str_non_empty() {
        let variants = [
            ConfigError::AlreadyInitialized,
            ConfigError::InvalidConfig,
            ConfigError::InvalidPhyAddress,
            ConfigError::ClockError,
            ConfigError::GpioError,
            ConfigError::ResetFailed,
        ];

        for variant in variants {
            let s = variant.as_str();
            assert!(!s.is_empty(), "ConfigError::{:?} has empty string", variant);
        }
    }

    #[test]
    fn config_error_display() {
        let err = ConfigError::InvalidPhyAddress;
        let display = format!("{}", err);
        assert_eq!(display, "invalid PHY address");
    }

    #[test]
    fn config_error_equality() {
        assert_eq!(ConfigError::ClockError, ConfigError::ClockError);
        assert_ne!(ConfigError::ClockError, ConfigError::GpioError);
    }

    #[test]
    fn config_error_clone() {
        let err = ConfigError::ResetFailed;
        let cloned = err.clone();
        assert_eq!(err, cloned);
    }

    // =========================================================================
    // DmaError Tests
    // =========================================================================

    #[test]
    fn dma_error_as_str_non_empty() {
        let variants = [
            DmaError::NoDescriptorsAvailable,
            DmaError::DescriptorBusy,
            DmaError::FrameTooLarge,
            DmaError::InvalidLength,
            DmaError::FatalBusError,
        ];

        for variant in variants {
            let s = variant.as_str();
            assert!(!s.is_empty(), "DmaError::{:?} has empty string", variant);
        }
    }

    #[test]
    fn dma_error_display() {
        let err = DmaError::NoDescriptorsAvailable;
        let display = format!("{}", err);
        assert_eq!(display, "no descriptors available");
    }

    #[test]
    fn dma_error_equality() {
        assert_eq!(DmaError::DescriptorBusy, DmaError::DescriptorBusy);
        assert_ne!(DmaError::DescriptorBusy, DmaError::FatalBusError);
    }

    // =========================================================================
    // IoError Tests
    // =========================================================================

    #[test]
    fn io_error_as_str_non_empty() {
        let variants = [
            IoError::Timeout,
            IoError::InvalidState,
            IoError::BufferTooSmall,
            IoError::IncompleteFrame,
            IoError::FrameError,
            IoError::PhyError,
        ];

        for variant in variants {
            let s = variant.as_str();
            assert!(!s.is_empty(), "IoError::{:?} has empty string", variant);
        }
    }

    #[test]
    fn io_error_display() {
        let err = IoError::Timeout;
        let display = format!("{}", err);
        assert_eq!(display, "operation timed out");
    }

    #[test]
    fn io_error_equality() {
        assert_eq!(IoError::PhyError, IoError::PhyError);
        assert_ne!(IoError::PhyError, IoError::Timeout);
    }

    // =========================================================================
    // Unified Error Tests
    // =========================================================================

    #[test]
    fn error_from_config_error() {
        let config_err = ConfigError::InvalidPhyAddress;
        let err: Error = config_err.into();

        match err {
            Error::Config(e) => assert_eq!(e, ConfigError::InvalidPhyAddress),
            _ => panic!("Expected Error::Config"),
        }
    }

    #[test]
    fn error_from_dma_error() {
        let dma_err = DmaError::NoDescriptorsAvailable;
        let err: Error = dma_err.into();

        match err {
            Error::Dma(e) => assert_eq!(e, DmaError::NoDescriptorsAvailable),
            _ => panic!("Expected Error::Dma"),
        }
    }

    #[test]
    fn error_from_io_error() {
        let io_err = IoError::Timeout;
        let err: Error = io_err.into();

        match err {
            Error::Io(e) => assert_eq!(e, IoError::Timeout),
            _ => panic!("Expected Error::Io"),
        }
    }

    #[test]
    fn error_display_config() {
        let err = Error::Config(ConfigError::ClockError);
        let display = format!("{}", err);
        assert!(display.contains("config"));
        assert!(display.contains("clock"));
    }

    #[test]
    fn error_display_dma() {
        let err = Error::Dma(DmaError::FatalBusError);
        let display = format!("{}", err);
        assert!(display.contains("dma"));
        assert!(display.contains("bus error"));
    }

    #[test]
    fn error_display_io() {
        let err = Error::Io(IoError::BufferTooSmall);
        let display = format!("{}", err);
        assert!(display.contains("io"));
        assert!(display.contains("buffer"));
    }

    #[test]
    fn error_equality() {
        let err1 = Error::Config(ConfigError::GpioError);
        let err2 = Error::Config(ConfigError::GpioError);
        let err3 = Error::Config(ConfigError::ClockError);

        assert_eq!(err1, err2);
        assert_ne!(err1, err3);
    }

    #[test]
    fn error_clone() {
        let err = Error::Io(IoError::FrameError);
        let cloned = err.clone();
        assert_eq!(err, cloned);
    }

    // =========================================================================
    // Result Type Alias Tests
    // =========================================================================

    #[test]
    fn result_type_works() {
        fn test_fn() -> Result<u32> {
            Ok(42)
        }

        assert_eq!(test_fn().unwrap(), 42);
    }

    #[test]
    fn config_result_type_works() {
        fn test_fn() -> ConfigResult<u32> {
            Err(ConfigError::InvalidConfig)
        }

        assert!(test_fn().is_err());
    }

    #[test]
    fn dma_result_type_works() {
        fn test_fn() -> DmaResult<u32> {
            Err(DmaError::InvalidLength)
        }

        assert!(test_fn().is_err());
    }

    #[test]
    fn io_result_type_works() {
        fn test_fn() -> IoResult<u32> {
            Err(IoError::Timeout)
        }

        assert!(test_fn().is_err());
    }
}
