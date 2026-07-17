use crate::monitor::Monitor;
use crate::proj::Proj;

#[derive(PartialEq, Clone, Copy)]
pub enum Section {
    Tabs,
    Settings,
    Layout,
}

#[derive(PartialEq, Clone)]
pub enum UiMode {
    Normal,
    ModeSelect { scroll: usize, hover: usize },
    TextEdit,
}

pub struct App {
    pub monitors: Vec<Monitor>,
    pub proj: Proj,
    pub tab: usize,
    pub section: Section,
    pub sel_field: usize,
    pub layout_sel: usize,
    pub ui: UiMode,
    pub buf: String,
    pub status: String,
    pub status_err: bool,
}

impl App {
    pub fn new(monitors: Vec<Monitor>) -> Self {
        App {
            monitors,
            proj: Proj::Custom,
            tab: 0,
            section: Section::Settings,
            sel_field: 0,
            layout_sel: 0,
            ui: UiMode::Normal,
            buf: String::new(),
            status: String::new(),
            status_err: false,
        }
    }

    pub fn set_status(&mut self, s: impl Into<String>, err: bool) {
        self.status = s.into();
        self.status_err = err;
    }

    pub fn cur_mon(&self) -> &Monitor {
        &self.monitors[self.tab]
    }
    pub fn cur_mon_mut(&mut self) -> &mut Monitor {
        &mut self.monitors[self.tab]
    }
}
