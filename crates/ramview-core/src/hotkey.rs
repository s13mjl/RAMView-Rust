// Win32 hotkey modifier flags (NativeMethods MOD_*).
pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;
pub const MOD_NOREPEAT: u32 = 0x4000;

/// Mirrors HotkeyService.ParseChord: parses "Ctrl+Shift+R" into (modifiers, virtual key).
pub fn parse_chord(chord: &str) -> (u32, u32) {
    let mut modifiers = 0u32;
    let mut vk = 0x52; // Default 'R'

    let chord = chord.trim();
    if chord.is_empty() {
        return (MOD_CONTROL | MOD_SHIFT, vk);
    }

    for part in chord.split('+') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        if part.eq_ignore_ascii_case("Ctrl") || part.eq_ignore_ascii_case("Control") {
            modifiers |= MOD_CONTROL;
        } else if part.eq_ignore_ascii_case("Shift") {
            modifiers |= MOD_SHIFT;
        } else if part.eq_ignore_ascii_case("Alt") {
            modifiers |= MOD_ALT;
        } else if part.eq_ignore_ascii_case("Win") || part.eq_ignore_ascii_case("Windows") {
            modifiers |= MOD_WIN;
        } else if part.chars().count() == 1 {
            let c = part.chars().next().unwrap().to_ascii_uppercase();
            if c.is_ascii_uppercase() || c.is_ascii_digit() {
                vk = c as u32;
            }
        }
    }

    (modifiers, vk)
}
