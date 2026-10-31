//! The built-in sample clip: a short make-believe take, so the editor can be tried (and
//! Vuoom's zooms seen) before recording anything. It goes through the same frame store, zoom
//! planner and pointer as a real take, so nothing about it is special once loaded.

use glam::DVec2;
use vuoom_zoom::{InputEvent, MouseButton};

pub const WIDTH: u32 = 1600;
pub const HEIGHT: u32 = 900;
pub const FPS: u32 = 30;
pub const SECONDS: u32 = 10;

const BACKDROP: [u8; 3] = [46, 38, 34];
const PANEL: [u8; 3] = [251, 250, 249];
const SIDEBAR: [u8; 3] = [240, 237, 235];
const INK: [u8; 3] = [62, 54, 50];
const SOFT: [u8; 3] = [200, 194, 190];
const ACCENT: [u8; 3] = [77, 72, 229];

/// Fill a rectangle of a BGRA image `WIDTH` wide with the BGR color `c`.
fn rect(px: &mut [u8], (x, y, w, h): (u32, u32, u32, u32), c: [u8; 3]) {
    let stride = WIDTH as usize * 4;
    let color = [c[0], c[1], c[2], 255];
    for row in y as usize..(y + h).min(HEIGHT) as usize {
        let at = row * stride + x as usize * 4;
        let end = (at + w as usize * 4).min((row + 1) * stride);
        px[at..end].as_chunks_mut::<4>().0.fill(color);
    }
}

/// A line of make-believe text: word-sized bars, `seed` choosing their lengths.
fn text_line(px: &mut [u8], (x, y): (u32, u32), width: u32, seed: u32, c: [u8; 3]) {
    let mut at = x;
    let mut k = seed;
    while at < x + width {
        k = k.wrapping_mul(2_654_435_761).wrapping_add(13);
        let word = 26 + (k >> 27) * 5;
        rect(px, (at, y, word.min(x + width - at), 12), c);
        at += word + 12;
    }
}

/// The sample's picture at time `t` (seconds): a window that fills in as the take goes on.
#[must_use]
pub fn frame(t: f64) -> Vec<u8> {
    let mut px = vec![0u8; WIDTH as usize * HEIGHT as usize * 4];
    rect(&mut px, (0, 0, WIDTH, HEIGHT), BACKDROP);
    rect(&mut px, (80, 60, 1440, 780), PANEL);
    rect(&mut px, (80, 60, 1440, 56), SIDEBAR);
    rect(&mut px, (80, 116, 300, 724), SIDEBAR);
    for (i, seed) in [3u32, 8, 21, 34, 55].iter().enumerate() {
        let y = 150 + i as u32 * 54;
        let on = (t / 2.0) as usize % 5 == i;
        if on {
            rect(&mut px, (96, y - 14, 268, 40), SOFT);
        }
        text_line(&mut px, (116, y), 200, *seed, INK);
    }
    // Lines of the document appear one after another, as if typed.
    let typed = (t * 34.0) as u32;
    for line in 0..14u32 {
        let width = typed.saturating_sub(line * 24).min(24) * 40;
        if width > 0 {
            let color = if line % 5 == 0 { ACCENT } else { INK };
            text_line(&mut px, (430, 170 + line * 40), width, line * 7 + 1, color);
        }
    }
    // A button that lights up once the pointer has pressed it.
    let pressed = t >= 6.2;
    let button = if pressed { ACCENT } else { SOFT };
    rect(&mut px, (1240, 740, 220, 60), button);
    text_line(&mut px, (1280, 764), 140, 5, PANEL);
    px
}

/// A pointer glide from `a` to `b` between two times.
fn glide(events: &mut Vec<InputEvent>, (t0, t1): (f64, f64), a: DVec2, b: DVec2) {
    let steps = ((t1 - t0) * 60.0) as u32;
    for k in 0..=steps {
        let u = f64::from(k) / f64::from(steps.max(1));
        let eased = u * u * (3.0 - 2.0 * u);
        events.push(InputEvent::Move {
            t: t0 + (t1 - t0) * u,
            pos: a.lerp(b, eased),
        });
    }
}

/// The sample's input log: the pointer moves around the window and clicks three things.
#[must_use]
pub fn events() -> Vec<InputEvent> {
    let (list, doc, button) = (
        DVec2::new(0.14, 0.26),
        DVec2::new(0.42, 0.34),
        DVec2::new(0.84, 0.85),
    );
    let mut log = Vec::new();
    glide(&mut log, (0.2, 1.0), DVec2::new(0.6, 0.6), list);
    log.push(InputEvent::Click {
        t: 1.2,
        pos: list,
        button: MouseButton::Left,
    });
    glide(&mut log, (2.4, 3.2), list, doc);
    log.push(InputEvent::Click {
        t: 3.4,
        pos: doc,
        button: MouseButton::Left,
    });
    glide(&mut log, (5.0, 6.0), doc, button);
    log.push(InputEvent::Click {
        t: 6.2,
        pos: button,
        button: MouseButton::Left,
    });
    log
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_is_the_size_it_says_and_opaque() {
        let px = frame(1.0);
        assert_eq!(px.len(), WIDTH as usize * HEIGHT as usize * 4);
        assert!(px.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    #[test]
    fn the_picture_changes_as_the_take_goes_on() {
        assert!(frame(0.5) != frame(4.0));
        // The button lights up when it is pressed.
        assert!(frame(6.0) != frame(6.4));
    }

    #[test]
    fn the_log_is_in_order_and_has_the_clicks() {
        let log = events();
        assert!(log.windows(2).all(|w| w[0].t() <= w[1].t()));
        let clicks = log.iter().filter(|e| e.is_click_trigger()).count();
        assert_eq!(clicks, 3);
    }
}
