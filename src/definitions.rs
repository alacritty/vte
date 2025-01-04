use core::mem;

#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Default, Copy, Clone)]
pub enum State {
    Anywhere = 0,
    CsiEntry = 1,
    CsiIgnore = 2,
    CsiIntermediate = 3,
    CsiParam = 4,
    DcsEntry = 5,
    DcsIgnore = 6,
    DcsIntermediate = 7,
    DcsParam = 8,
    DcsPassthrough = 9,
    Escape = 10,
    EscapeIntermediate = 11,
    #[default]
    Ground = 12,
    OscString = 13,
    OpaqueString = 14,
    Utf8 = 15,
}

#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum Action {
    None = 0,
    Collect = 1,
    CsiDispatch = 2,
    EscDispatch = 3,
    Execute = 4,
    Ignore = 5,
    OscPut = 6,
    Param = 7,
    Print = 8,
    Put = 9,
    BeginUtf8 = 10,
    OpaquePut = 11,

    // Actions that do not need to be packed as 4 bits in the state table
    // can have values higher than 16.
    Clear = 16,
    Hook = 17,
    Unhook = 18,
    OscStart = 19,
    OscEnd = 20,
    OpaqueStart = 21,
    OpaqueEnd = 22,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum OpaqueSequenceKind {
    Sos,
    Pm,
    Apc,
}

/// Unpack a u8 into a State and Action
///
/// The implementation of this assumes that there are *precisely* 16 variants for both Action and
/// State. Furthermore, it assumes that the enums are tag-only; that is, there is no data in any
/// variant.
///
/// Bad things will happen if those invariants are violated.
#[inline(always)]
pub fn unpack(delta: u8) -> (State, Action) {
    unsafe {
        (
            // State is stored in bottom 4 bits
            mem::transmute::<u8, State>(delta & 0x0f),
            // Action is stored in top 4 bits
            mem::transmute::<u8, Action>(delta >> 4),
        )
    }
}

#[inline(always)]
pub const fn pack(state: State, action: Action) -> u8 {
    (action as u8) << 4 | state as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpack_state_action() {
        match unpack(0xaa) {
            (State::Escape, Action::BeginUtf8) => (),
            _ => panic!("unpack failed"),
        }

        match unpack(0x0f) {
            (State::Utf8, Action::None) => (),
            _ => panic!("unpack failed"),
        }

        match unpack(0xbf) {
            (State::Utf8, Action::OpaquePut) => (),
            _ => panic!("unpack failed"),
        }
    }

    #[test]
    fn pack_state_action() {
        assert_eq!(pack(State::Escape, Action::BeginUtf8), 0xaa);
        assert_eq!(pack(State::Utf8, Action::None), 0x0f);
        assert_eq!(pack(State::Utf8, Action::OpaquePut), 0xbf);
    }
}
