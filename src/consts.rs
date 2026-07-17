pub const CP_NORMAL: i16 = 1;
pub const CP_HEADER: i16 = 2;
pub const CP_SELECTED: i16 = 3;
pub const CP_MODIFIED: i16 = 4;
pub const CP_STATUSBAR: i16 = 5;
pub const CP_ERROR: i16 = 6;
pub const CP_OVERLAY: i16 = 7;

pub const TRANSFORMS: &[&str] = &[
    "normal",
    "90",
    "180",
    "270",
    "flipped",
    "flipped-90",
    "flipped-180",
    "flipped-270",
];

pub const N_FIELDS: usize = 5;
pub const F_MODE: usize = 0;
pub const F_SCALE: usize = 1;
pub const F_TRANSFORM: usize = 2;
pub const F_ADAPTIVE: usize = 3;
pub const F_ENABLED: usize = 4;
pub const FIELD_NAMES: &[&str] = &["Mode", "Scale", "Transform", "Adaptive Sync", "Enabled"];
