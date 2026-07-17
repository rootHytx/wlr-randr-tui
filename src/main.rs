mod app;
mod apply;
mod consts;
mod input;
mod monitor;
mod parse;
mod proj;
mod ui;

use ncurses::*;

use app::App;
use consts::*;
use input::handle;
use parse::parse_monitors;
use ui::draw;

fn main() {
    let monitors = parse_monitors();
    if monitors.is_empty() {
        eprintln!("No monitors detected.");
        std::process::exit(1);
    }

    initscr();
    start_color();
    use_default_colors();
    cbreak();
    noecho();
    keypad(stdscr(), true);
    curs_set(CURSOR_VISIBILITY::CURSOR_INVISIBLE);

    init_pair(CP_NORMAL,    -1,          -1);
    init_pair(CP_HEADER,    COLOR_BLACK, COLOR_CYAN);
    init_pair(CP_SELECTED,  COLOR_BLACK, COLOR_GREEN);
    init_pair(CP_MODIFIED,  COLOR_YELLOW, -1);
    init_pair(CP_STATUSBAR, COLOR_BLACK, COLOR_WHITE);
    init_pair(CP_ERROR,     COLOR_WHITE, COLOR_RED);
    init_pair(CP_OVERLAY,   COLOR_BLACK, COLOR_MAGENTA);

    let mut app = App::new(monitors);
    loop {
        draw(&app);
        let ch = getch();
        if !handle(&mut app, ch) { break; }
    }
    endwin();
}
