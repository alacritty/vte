#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
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
    SosPmString = 14,
    Utf8 = 15,
    ApcString = 16,
}

#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Action {
    None = 0,
    Clear = 1,
    Collect = 2,
    CsiDispatch = 3,
    EscDispatch = 4,
    Execute = 5,
    Hook = 6,
    Ignore = 7,
    OscEnd = 8,
    OscPut = 9,
    OscStart = 10,
    Param = 11,
    Print = 12,
    Put = 13,
    Unhook = 14,
    BeginUtf8 = 15,
    ApcBegin = 16,
    ApcEnd = 17,
    ApcPut = 18,
}
