use crate::{app::App, world};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Stylize,
    text::Line,
    widgets::{Block, BorderType, canvas::Canvas},
};

/// ```text
///  _cubes______________
/// |                    |
/// |                    |
/// |                    |
/// |____________________|
/// |____________________|
/// ```
pub fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(f.area());

    // 2 blocks less: border
    // let new_area = Area::new(
    //     (chunks[0].width - 2) * BRAILLE.width,
    //     (chunks[0].height - 2) * BRAILLE.height,
    // );

    let views = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    let title = Block::bordered()
        .border_type(BorderType::Thick)
        .title(format!("xz"));
    let xz = Canvas::default()
        // .x_bounds([0., chunks[0].height as f64 * 2. - 4.])
        // .y_bounds([0., chunks[0].height as f64 * 2. - 4.])
        .paint(|ctx| ctx.draw(&world::ProjectXZ(&app.world)))
        .block(title);
    f.render_widget(xz, views[0]);

    let title = Block::bordered()
        .border_type(BorderType::Thick)
        .title(format!("yz"));
    let yz = Canvas::default()
        // .x_bounds([0., chunks[0].height as f64 * 2. - 4.])
        // .y_bounds([0., chunks[0].height as f64 * 2. - 4.])
        .paint(|ctx| ctx.draw(&world::ProjectYZ(&app.world)))
        .block(title);
    f.render_widget(yz, views[1]);

    let footer = Layout::horizontal([Constraint::Fill(1)]).split(chunks[1]);

    let title = "bigworlds [cubes]".dark_gray();
    let current_keys_hint = "[q]uit".yellow();

    let poll_t = {
        if let crate::app::PAUSE = app.poll_t {
            "paused".into()
        } else {
            format!("Poll time: {:.0?}", app.poll_t)
        }
    }
    .light_blue();

    let div = " | ".white();
    let current_stats = vec![title, div.clone(), current_keys_hint, div, poll_t];
    let footer_data = Line::from(current_stats);

    f.render_widget(footer_data, footer[0]);
}
