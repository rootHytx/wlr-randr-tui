mod app;
mod apply;
mod consts;
mod input;
mod monitor;
mod parse;
mod persist;
mod proj;
mod ui;

use ncurses::*;

use app::App;
use apply::do_apply;
use consts::*;
use input::handle;
use parse::parse_monitors;
use ui::draw;

fn main() {
    let no_restore = std::env::args().any(|a| a == "--no-restore" || a == "-n");
    let mut monitors = parse_monitors();
    if monitors.is_empty() {
        eprintln!("No monitors detected.");
        std::process::exit(1);
    }

    // Re-apply the profiles saved by the last run before drawing anything.
    let mut startup_status = String::new();
    let mut startup_err = false;
    if !no_restore {
        if let Some(path) = persist::config_path() {
            let saved = persist::load(&path);
            let restored = persist::restore(&mut monitors, &saved);
            if restored > 0 {
                match do_apply(&monitors) {
                    Ok(msg) => {
                        monitors = parse_monitors();
                        startup_status = if msg == "No changes to apply" {
                            format!("Saved profile matched {} output(s)", restored)
                        } else {
                            format!("Restored saved profile for {} output(s)", restored)
                        };
                    }
                    Err(e) => {
                        startup_status = format!("Restore failed: {}", e);
                        startup_err = true;
                    }
                }
            }
        }
    }

    initscr();
    start_color();
    use_default_colors();
    cbreak();
    noecho();
    keypad(stdscr(), true);
    curs_set(CURSOR_VISIBILITY::CURSOR_INVISIBLE);

    init_pair(CP_NORMAL, -1, -1);
    init_pair(CP_HEADER, COLOR_BLACK, COLOR_CYAN);
    init_pair(CP_SELECTED, COLOR_BLACK, COLOR_GREEN);
    init_pair(CP_MODIFIED, COLOR_YELLOW, -1);
    init_pair(CP_STATUSBAR, COLOR_BLACK, COLOR_WHITE);
    init_pair(CP_ERROR, COLOR_WHITE, COLOR_RED);
    init_pair(CP_OVERLAY, COLOR_BLACK, COLOR_MAGENTA);

    let mut app = App::new(monitors);
    if !startup_status.is_empty() {
        app.set_status(startup_status, startup_err);
    }
    loop {
        draw(&app);
        let ch = getch();
        if !handle(&mut app, ch) {
            break;
        }
    }
    endwin();
}
