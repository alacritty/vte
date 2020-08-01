use std::string::String;

use log::trace;
use serde::{Deserialize, Serialize};

type Line = usize;
type Column = usize;

#[derive(Debug, Eq, PartialEq, Copy, Clone, Default, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Type that handles actions from the parser.
///
/// XXX Should probably not provide default impls for everything, but it makes
/// writing specific handler impls for tests far easier.
pub trait Handler<W> {
    /// OSC to set window title.
    fn set_title(&mut self, _: Option<String>) {}

    /// Set the cursor style.
    fn set_cursor_style(&mut self, _: Option<CursorStyle>) {}

    /// A character to be displayed.
    fn input(&mut self, _c: char) {}

    /// Set cursor to position.
    fn goto(&mut self, _: Line, _: Column) {}

    /// Set cursor to specific row.
    fn goto_line(&mut self, _: Line) {}

    /// Set cursor to specific column.
    fn goto_col(&mut self, _: Column) {}

    /// Insert blank characters in current line starting from cursor.
    fn insert_blank(&mut self, _: Column) {}

    /// Move cursor up `rows`.
    fn move_up(&mut self, _: Line) {}

    /// Move cursor down `rows`.
    fn move_down(&mut self, _: Line) {}

    /// Identify the terminal (should write back to the pty stream).
    ///
    /// TODO this should probably return an io::Result
    fn identify_terminal(&mut self, _: &mut W, _intermediate: Option<char>) {}

    /// Report device status.
    fn device_status(&mut self, _: &mut W, _: usize) {}

    /// Move cursor forward `cols`.
    fn move_forward(&mut self, _: Column) {}

    /// Move cursor backward `cols`.
    fn move_backward(&mut self, _: Column) {}

    /// Move cursor down `rows` and set to column 1.
    fn move_down_and_cr(&mut self, _: Line) {}

    /// Move cursor up `rows` and set to column 1.
    fn move_up_and_cr(&mut self, _: Line) {}

    /// Put `count` tabs.
    fn put_tab(&mut self, _count: i64) {}

    /// Backspace `count` characters.
    fn backspace(&mut self) {}

    /// Carriage return.
    fn carriage_return(&mut self) {}

    /// Linefeed.
    fn linefeed(&mut self) {}

    /// Ring the bell.
    ///
    /// Hopefully this is never implemented.
    fn bell(&mut self) {}

    /// Substitute char under cursor.
    fn substitute(&mut self) {}

    /// Newline.
    fn newline(&mut self) {}

    /// Set current position as a tabstop.
    fn set_horizontal_tabstop(&mut self) {}

    /// Scroll up `rows` rows.
    fn scroll_up(&mut self, _: Line) {}

    /// Scroll down `rows` rows.
    fn scroll_down(&mut self, _: Line) {}

    /// Insert `count` blank lines.
    fn insert_blank_lines(&mut self, _: Line) {}

    /// Delete `count` lines.
    fn delete_lines(&mut self, _: Line) {}

    /// Erase `count` chars in current line following cursor.
    ///
    /// Erase means resetting to the default state (default colors, no content,
    /// no mode flags).
    fn erase_chars(&mut self, _: Column) {}

    /// Delete `count` chars.
    ///
    /// Deleting a character is like the delete key on the keyboard - everything
    /// to the right of the deleted things is shifted left.
    fn delete_chars(&mut self, _: Column) {}

    /// Move backward `count` tabs.
    fn move_backward_tabs(&mut self, _count: i64) {}

    /// Move forward `count` tabs.
    fn move_forward_tabs(&mut self, _count: i64) {}

    /// Save current cursor position.
    fn save_cursor_position(&mut self) {}

    /// Restore cursor position.
    fn restore_cursor_position(&mut self) {}

    /// Clear current line.
    fn clear_line(&mut self, _mode: LineClearMode) {}

    /// Clear screen.
    fn clear_screen(&mut self, _mode: ClearMode) {}

    /// Clear tab stops.
    fn clear_tabs(&mut self, _mode: TabulationClearMode) {}

    /// Reset terminal state.
    fn reset_state(&mut self) {}

    /// Reverse Index.
    ///
    /// Move the active position to the same horizontal position on the
    /// preceding line. If the active position is at the top margin, a scroll
    /// down is performed.
    fn reverse_index(&mut self) {}

    /// Set a terminal attribute.
    fn terminal_attribute(&mut self, _attr: Attr) {}

    /// Set mode.
    fn set_mode(&mut self, _mode: Mode) {}

    /// Unset mode.
    fn unset_mode(&mut self, _: Mode) {}

    /// DECSTBM - Set the terminal scrolling region.
    fn set_scrolling_region(&mut self, _top: usize, _bottom: Option<usize>) {}

    /// DECKPAM - Set keypad to applications mode (ESCape instead of digits).
    fn set_keypad_application_mode(&mut self) {}

    /// DECKPNM - Set keypad to numeric mode (digits instead of ESCape seq).
    fn unset_keypad_application_mode(&mut self) {}

    /// Set one of the graphic character sets, G0 to G3, as the active charset.
    ///
    /// 'Invoke' one of G0 to G3 in the GL area. Also referred to as shift in,
    /// shift out and locking shift depending on the set being activated.
    fn set_active_charset(&mut self, _: CharsetIndex) {}

    /// Assign a graphic character set to G0, G1, G2 or G3.
    ///
    /// 'Designate' a graphic character set as one of G0 to G3, so that it can
    /// later be 'invoked' by `set_active_charset`.
    fn configure_charset(&mut self, _: CharsetIndex, _: StandardCharset) {}

    /// Set an indexed color value.
    fn set_color(&mut self, _: usize, _: Rgb) {}

    /// Write a foreground/background color escape sequence with the current color.
    fn dynamic_color_sequence(&mut self, _: &mut W, _: u8, _: usize, _: &str) {}

    /// Reset an indexed color to original value.
    fn reset_color(&mut self, _: usize) {}

    /// Store data into clipboard.
    fn clipboard_store(&mut self, _: u8, _: &[u8]) {}

    /// Load data from clipboard.
    fn clipboard_load(&mut self, _: u8, _: &str) {}

    /// Run the decaln routine.
    fn decaln(&mut self) {}

    /// Push a title onto the stack.
    fn push_title(&mut self) {}

    /// Pop the last title from the stack.
    fn pop_title(&mut self) {}
}

/// Describes shape of cursor.
#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash, Deserialize)]
pub enum CursorStyle {
    /// Cursor is a block like `▒`.
    Block,

    /// Cursor is an underscore like `_`.
    Underline,

    /// Cursor is a vertical bar `⎸`.
    Beam,

    /// Cursor is a box like `☐`.
    #[serde(skip)]
    HollowBlock,

    /// Invisible cursor.
    #[serde(skip)]
    Hidden,
}

impl Default for CursorStyle {
    fn default() -> CursorStyle {
        CursorStyle::Block
    }
}

/// Terminal modes.
#[derive(Debug, Eq, PartialEq)]
pub enum Mode {
    /// ?1
    CursorKeys = 1,
    /// Select 80 or 132 columns per page.
    ///
    /// CSI ? 3 h -> set 132 column font.
    /// CSI ? 3 l -> reset 80 column font.
    ///
    /// Additionally,
    ///
    /// * set margins to default positions
    /// * erases all data in page memory
    /// * resets DECLRMM to unavailable
    /// * clears data from the status line (if set to host-writable)
    DECCOLM = 3,
    /// IRM Insert Mode.
    ///
    /// NB should be part of non-private mode enum.
    ///
    /// * `CSI 4 h` change to insert mode
    /// * `CSI 4 l` reset to replacement mode
    Insert = 4,
    /// ?6
    Origin = 6,
    /// ?7
    LineWrap = 7,
    /// ?12
    BlinkingCursor = 12,
    /// 20
    ///
    /// NB This is actually a private mode. We should consider adding a second
    /// enumeration for public/private modesets.
    LineFeedNewLine = 20,
    /// ?25
    ShowCursor = 25,
    /// ?1000
    ReportMouseClicks = 1000,
    /// ?1002
    ReportCellMouseMotion = 1002,
    /// ?1003
    ReportAllMouseMotion = 1003,
    /// ?1004
    ReportFocusInOut = 1004,
    /// ?1005
    Utf8Mouse = 1005,
    /// ?1006
    SgrMouse = 1006,
    /// ?1007
    AlternateScroll = 1007,
    /// ?1049
    SwapScreenAndSetRestoreCursor = 1049,
    /// ?2004
    BracketedPaste = 2004,
}

impl Mode {
    /// Create mode from a primitive.
    ///
    /// TODO lots of unhandled values.
    pub fn from_primitive(intermediate: Option<&u8>, num: i64) -> Option<Mode> {
        let private = match intermediate {
            Some(b'?') => true,
            None => false,
            _ => return None,
        };

        if private {
            Some(match num {
                1 => Mode::CursorKeys,
                3 => Mode::DECCOLM,
                6 => Mode::Origin,
                7 => Mode::LineWrap,
                12 => Mode::BlinkingCursor,
                25 => Mode::ShowCursor,
                1000 => Mode::ReportMouseClicks,
                1002 => Mode::ReportCellMouseMotion,
                1003 => Mode::ReportAllMouseMotion,
                1004 => Mode::ReportFocusInOut,
                1005 => Mode::Utf8Mouse,
                1006 => Mode::SgrMouse,
                1007 => Mode::AlternateScroll,
                1049 => Mode::SwapScreenAndSetRestoreCursor,
                2004 => Mode::BracketedPaste,
                _ => {
                    trace!("[unimplemented] primitive mode: {}", num);
                    return None;
                },
            })
        } else {
            Some(match num {
                4 => Mode::Insert,
                20 => Mode::LineFeedNewLine,
                _ => return None,
            })
        }
    }
}

/// Mode for clearing line.
///
/// Relative to cursor.
#[derive(Debug)]
pub enum LineClearMode {
    /// Clear right of cursor.
    Right,
    /// Clear left of cursor.
    Left,
    /// Clear entire line.
    All,
}

/// Mode for clearing terminal.
///
/// Relative to cursor.
#[derive(Debug)]
pub enum ClearMode {
    /// Clear below cursor.
    Below,
    /// Clear above cursor.
    Above,
    /// Clear entire terminal.
    All,
    /// Clear 'saved' lines (scrollback).
    Saved,
}

/// Mode for clearing tab stops.
#[derive(Debug)]
pub enum TabulationClearMode {
    /// Clear stop under cursor.
    Current,
    /// Clear all stops.
    All,
}

/// Standard colors.
///
/// The order here matters since the enum should be castable to a `usize` for
/// indexing a color list.
#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NamedColor {
    /// Black.
    Black = 0,
    /// Red.
    Red,
    /// Green.
    Green,
    /// Yellow.
    Yellow,
    /// Blue.
    Blue,
    /// Magenta.
    Magenta,
    /// Cyan.
    Cyan,
    /// White.
    White,
    /// Bright black.
    BrightBlack,
    /// Bright red.
    BrightRed,
    /// Bright green.
    BrightGreen,
    /// Bright yellow.
    BrightYellow,
    /// Bright blue.
    BrightBlue,
    /// Bright magenta.
    BrightMagenta,
    /// Bright cyan.
    BrightCyan,
    /// Bright white.
    BrightWhite,
    /// The foreground color.
    Foreground = 256,
    /// The background color.
    Background,
    /// Color for the cursor itself.
    Cursor,
    /// Dim black.
    DimBlack,
    /// Dim red.
    DimRed,
    /// Dim green.
    DimGreen,
    /// Dim yellow.
    DimYellow,
    /// Dim blue.
    DimBlue,
    /// Dim magenta.
    DimMagenta,
    /// Dim cyan.
    DimCyan,
    /// Dim white.
    DimWhite,
    /// The bright foreground color.
    BrightForeground,
    /// Dim foreground.
    DimForeground,
}

impl NamedColor {
    pub fn to_bright(self) -> Self {
        match self {
            NamedColor::Foreground => NamedColor::BrightForeground,
            NamedColor::Black => NamedColor::BrightBlack,
            NamedColor::Red => NamedColor::BrightRed,
            NamedColor::Green => NamedColor::BrightGreen,
            NamedColor::Yellow => NamedColor::BrightYellow,
            NamedColor::Blue => NamedColor::BrightBlue,
            NamedColor::Magenta => NamedColor::BrightMagenta,
            NamedColor::Cyan => NamedColor::BrightCyan,
            NamedColor::White => NamedColor::BrightWhite,
            NamedColor::DimForeground => NamedColor::Foreground,
            NamedColor::DimBlack => NamedColor::Black,
            NamedColor::DimRed => NamedColor::Red,
            NamedColor::DimGreen => NamedColor::Green,
            NamedColor::DimYellow => NamedColor::Yellow,
            NamedColor::DimBlue => NamedColor::Blue,
            NamedColor::DimMagenta => NamedColor::Magenta,
            NamedColor::DimCyan => NamedColor::Cyan,
            NamedColor::DimWhite => NamedColor::White,
            val => val,
        }
    }

    pub fn to_dim(self) -> Self {
        match self {
            NamedColor::Black => NamedColor::DimBlack,
            NamedColor::Red => NamedColor::DimRed,
            NamedColor::Green => NamedColor::DimGreen,
            NamedColor::Yellow => NamedColor::DimYellow,
            NamedColor::Blue => NamedColor::DimBlue,
            NamedColor::Magenta => NamedColor::DimMagenta,
            NamedColor::Cyan => NamedColor::DimCyan,
            NamedColor::White => NamedColor::DimWhite,
            NamedColor::Foreground => NamedColor::DimForeground,
            NamedColor::BrightBlack => NamedColor::Black,
            NamedColor::BrightRed => NamedColor::Red,
            NamedColor::BrightGreen => NamedColor::Green,
            NamedColor::BrightYellow => NamedColor::Yellow,
            NamedColor::BrightBlue => NamedColor::Blue,
            NamedColor::BrightMagenta => NamedColor::Magenta,
            NamedColor::BrightCyan => NamedColor::Cyan,
            NamedColor::BrightWhite => NamedColor::White,
            NamedColor::BrightForeground => NamedColor::Foreground,
            val => val,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    Named(NamedColor),
    Spec(Rgb),
    Indexed(u8),
}

/// Terminal character attributes.
#[derive(Debug, Eq, PartialEq)]
pub enum Attr {
    /// Clear all special abilities.
    Reset,
    /// Bold text.
    Bold,
    /// Dim or secondary color.
    Dim,
    /// Italic text.
    Italic,
    /// Underline text.
    Underline,
    /// Blink cursor slowly.
    BlinkSlow,
    /// Blink cursor fast.
    BlinkFast,
    /// Invert colors.
    Reverse,
    /// Do not display characters.
    Hidden,
    /// Strikeout text.
    Strike,
    /// Cancel bold.
    CancelBold,
    /// Cancel bold and dim.
    CancelBoldDim,
    /// Cancel italic.
    CancelItalic,
    /// Cancel underline.
    CancelUnderline,
    /// Cancel blink.
    CancelBlink,
    /// Cancel inversion.
    CancelReverse,
    /// Cancel text hiding.
    CancelHidden,
    /// Cancel strikeout.
    CancelStrike,
    /// Set indexed foreground color.
    Foreground(Color),
    /// Set indexed background color.
    Background(Color),
}

/// Identifiers which can be assigned to a graphic character set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharsetIndex {
    /// Default set, is designated as ASCII at startup.
    G0,
    G1,
    G2,
    G3,
}

impl Default for CharsetIndex {
    fn default() -> Self {
        CharsetIndex::G0
    }
}

/// Standard or common character sets which can be designated as G0-G3.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardCharset {
    Ascii,
    SpecialCharacterAndLineDrawing,
}

impl Default for StandardCharset {
    fn default() -> Self {
        StandardCharset::Ascii
    }
}

impl StandardCharset {
    /// Switch/Map character to the active charset. Ascii is the common case and
    /// for that we want to do as little as possible.
    #[inline]
    pub fn map(self, c: char) -> char {
        match self {
            StandardCharset::Ascii => c,
            StandardCharset::SpecialCharacterAndLineDrawing => match c {
                '`' => '◆',
                'a' => '▒',
                'b' => '\t',
                'c' => '\u{000c}',
                'd' => '\r',
                'e' => '\n',
                'f' => '°',
                'g' => '±',
                'h' => '\u{2424}',
                'i' => '\u{000b}',
                'j' => '┘',
                'k' => '┐',
                'l' => '┌',
                'm' => '└',
                'n' => '┼',
                'o' => '⎺',
                'p' => '⎻',
                'q' => '─',
                'r' => '⎼',
                's' => '⎽',
                't' => '├',
                'u' => '┤',
                'v' => '┴',
                'w' => '┬',
                'x' => '│',
                'y' => '≤',
                'z' => '≥',
                '{' => 'π',
                '|' => '≠',
                '}' => '£',
                '~' => '·',
                _ => c,
            },
        }
    }
}
