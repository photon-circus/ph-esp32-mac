//! Strict decoder for deterministic host-generated Ethernet stimulus.

/// Experimental EtherType reserved for the PHQA harness.
pub const PHQA_ETHERTYPE: u16 = 0x88b5;

/// MDIO suite code.
pub const SUITE_MDIO: u8 = 1;
/// Reset suite code.
pub const SUITE_RESET: u8 = 2;
/// MAC-filter suite code.
pub const SUITE_MAC_FILTER: u8 = 3;
/// Synchronous RX suite code.
pub const SUITE_RX_SYNC: u8 = 4;
/// Raw-async RX suite code.
pub const SUITE_RX_ASYNC: u8 = 5;
/// embassy-net RX suite code.
pub const SUITE_RX_EMBASSY: u8 = 6;

/// Link-down fixture action.
pub const ACTION_LINK_DOWN: u8 = 1;
/// Link-up fixture action.
pub const ACTION_LINK_UP: u8 = 2;
/// Device-unicast injection action.
pub const ACTION_INJECT_UNICAST: u8 = 3;
/// Broadcast injection action.
pub const ACTION_INJECT_BROADCAST: u8 = 4;
/// Configured-multicast injection action.
pub const ACTION_INJECT_MULTICAST: u8 = 5;
/// Alien-unicast injection action.
pub const ACTION_INJECT_ALIEN: u8 = 6;
/// Former-device-address injection action.
pub const ACTION_INJECT_OLD_MAC: u8 = 7;
/// New-device-address injection action.
pub const ACTION_INJECT_NEW_MAC: u8 = 8;
/// RX-ring fill action.
pub const ACTION_FILL_RX: u8 = 9;
/// RX flood action.
pub const ACTION_FLOOD_RX: u8 = 10;
/// Recovery sentinel action.
pub const ACTION_SEND_SENTINEL: u8 = 11;
/// Second async-wake sentinel action.
pub const ACTION_SEND_SECOND_SENTINEL: u8 = 12;
/// UDP echo action.
pub const ACTION_UDP_ECHO: u8 = 13;
/// DHCP fixture action.
pub const ACTION_DHCP: u8 = 14;

const FRAME_LEN: usize = 64;
const RUN_FIELD_LEN: usize = 16;

/// Identity fields that bind one stimulus to the active firmware request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StimulusIdentity<'a> {
    /// Expected Ethernet destination address.
    pub destination: [u8; 6],
    /// Suite code embedded in the frame.
    pub suite: u8,
    /// Action code embedded in the frame.
    pub action: u8,
    /// Lowercase hexadecimal run identifier compiled into the firmware.
    pub run_id: &'a str,
    /// Monotonic `READY` step that authorized the stimulus.
    pub step: u32,
}

/// Decode a sequence number only when every stimulus identity field matches.
///
/// The frame must be exactly 64 bytes and use this layout:
///
/// - bytes 12..14: EtherType `0x88b5`;
/// - bytes 14..18: `PHQA`;
/// - byte 18: protocol version `1`;
/// - bytes 19 and 20: suite and action codes;
/// - byte 21 and bytes 22..38: run-ID length and zero-padded ASCII value;
/// - bytes 38..42 and 42..46: big-endian `READY` step and sequence;
/// - bytes 46..64: zero.
#[must_use]
pub fn decode_stimulus(frame: &[u8], expected: StimulusIdentity<'_>) -> Option<u32> {
    if frame.len() != FRAME_LEN
        || frame[..6] != expected.destination
        || u16::from_be_bytes([frame[12], frame[13]]) != PHQA_ETHERTYPE
        || &frame[14..18] != b"PHQA"
        || frame[18] != 1
        || frame[19] != expected.suite
        || frame[20] != expected.action
    {
        return None;
    }

    let run_len = usize::from(frame[21]);
    let expected_run = expected.run_id.as_bytes();
    if run_len == 0
        || run_len > RUN_FIELD_LEN
        || expected_run.len() != run_len
        || !expected_run
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        || &frame[22..22 + run_len] != expected_run
        || frame[22 + run_len..38].iter().any(|byte| *byte != 0)
        || frame[46..].iter().any(|byte| *byte != 0)
    {
        return None;
    }

    let step = u32::from_be_bytes(frame[38..42].try_into().ok()?);
    if step != expected.step {
        return None;
    }

    Some(u32::from_be_bytes(frame[42..46].try_into().ok()?))
}
