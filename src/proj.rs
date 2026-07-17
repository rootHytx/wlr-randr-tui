use crate::monitor::{Align, Monitor, PosMode};

#[derive(Clone, Copy, PartialEq)]
pub enum Proj {
    Extend,
    ExtOnly,
    LaptopOnly,
    Custom,
}

impl Proj {
    pub const ALL: &'static [Proj] = &[Proj::Extend, Proj::ExtOnly, Proj::LaptopOnly, Proj::Custom];

    pub fn label(self) -> &'static str {
        match self {
            Proj::Extend => "Extend",
            Proj::ExtOnly => "Ext Only",
            Proj::LaptopOnly => "Laptop Only",
            Proj::Custom => "Custom",
        }
    }

    pub fn step(self, d: i32) -> Self {
        let i = Self::ALL.iter().position(|&p| p == self).unwrap_or(0);
        Self::ALL[((i as i32 + d).rem_euclid(Self::ALL.len() as i32)) as usize]
    }
}

pub fn apply_proj(mons: &mut [Monitor], proj: Proj) {
    let first_ext = mons.iter().find(|m| !m.is_laptop()).map(|m| m.name.clone());
    match proj {
        Proj::Extend => {
            for m in mons.iter_mut() {
                m.new_enabled = true;
            }
            for m in mons.iter_mut() {
                if !m.is_laptop() {
                    m.new_pos = PosMode::Absolute(0, 0);
                }
            }
            if let Some(ext) = &first_ext {
                for m in mons.iter_mut() {
                    if m.is_laptop() {
                        m.new_pos = PosMode::Below(ext.clone(), Align::Center);
                    }
                }
            }
        }
        Proj::ExtOnly => {
            for m in mons.iter_mut() {
                m.new_enabled = !m.is_laptop();
            }
        }
        Proj::LaptopOnly => {
            for m in mons.iter_mut() {
                m.new_enabled = m.is_laptop();
            }
        }
        Proj::Custom => {}
    }
}
