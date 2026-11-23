//! Generates the Crossroads app icon: `cargo run --example gen_icon`.
//!
//! A Fluent-style tile (teal → blue gradient) with a white path that forks
//! into two arrows — one link, several destinations. Each ICO size is rendered
//! natively rather than downscaled, so small sizes stay crisp.
//! Writes `assets/app.ico` (embedded by build.rs) and `assets/app.png`.

use tiny_skia::{
    Color, FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Paint, Path, PathBuilder, Pixmap,
    Point, SpreadMode, Stroke, Transform,
};

const SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];

fn rounded_rect(x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> Path {
    let k = r * 0.5523;
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + r, y0);
    pb.line_to(x1 - r, y0);
    pb.cubic_to(x1 - r + k, y0, x1, y0 + r - k, x1, y0 + r);
    pb.line_to(x1, y1 - r);
    pb.cubic_to(x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1);
    pb.line_to(x0 + r, y1);
    pb.cubic_to(x0 + r - k, y1, x0, y1 - r + k, x0, y1 - r);
    pb.line_to(x0, y0 + r);
    pb.cubic_to(x0, y0 + r - k, x0 + r - k, y0, x0 + r, y0);
    pb.close();
    pb.finish().unwrap()
}

fn solid(r: u8, g: u8, b: u8, a: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(Color::from_rgba8(r, g, b, (a * 255.) as u8));
    p.anti_alias = true;
    p
}

fn gradient() -> Paint<'static> {
    let shader = LinearGradient::new(
        Point::from_xy(24., 16.),
        Point::from_xy(232., 240.),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(0x2E, 0xC4, 0xB6, 255)),
            GradientStop::new(1.0, Color::from_rgba8(0x25, 0x5F, 0xD8, 255)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap();
    Paint { shader, anti_alias: true, ..Paint::default() }
}

/// Arrow head (filled triangle) at `tip`, pointing along (dx, dy).
fn arrow_head(tip: (f32, f32), dir: (f32, f32), len: f32, half: f32) -> Path {
    let n = (dir.0 * dir.0 + dir.1 * dir.1).sqrt();
    let (ux, uy) = (dir.0 / n, dir.1 / n);
    let (bx, by) = (tip.0 - ux * len, tip.1 - uy * len);
    let mut pb = PathBuilder::new();
    pb.move_to(tip.0, tip.1);
    pb.line_to(bx - uy * half, by + ux * half);
    pb.line_to(bx + uy * half, by - ux * half);
    pb.close();
    pb.finish().unwrap()
}

fn render(size: u32) -> Pixmap {
    let mut pm = Pixmap::new(size, size).unwrap();
    let t = Transform::from_scale(size as f32 / 256., size as f32 / 256.);
    let small = size <= 24;
    let (inset, radius) = if small { (8., 48.) } else { (16., 52.) };
    pm.fill_path(&rounded_rect(inset, inset, 256. - inset, 256. - inset, radius), &gradient(), FillRule::Winding, t, None);

    let white = solid(255, 255, 255, 1.0);
    let faded = solid(255, 255, 255, 0.6);
    let stroke = Stroke {
        width: if small { 30. } else { 22. },
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    let (head_len, head_half) = if small { (52., 40.) } else { (44., 32.) };

    // Stem from the bottom up to the fork.
    let mut stem = PathBuilder::new();
    stem.move_to(128., 210.);
    stem.line_to(128., 140.);
    // Left branch (faded: the road not taken).
    let mut left = PathBuilder::new();
    left.move_to(128., 140.);
    left.cubic_to(128., 110., 96., 100., 82., 86.);
    // Right branch (bright: where the link goes).
    let mut right = PathBuilder::new();
    right.move_to(128., 140.);
    right.cubic_to(128., 110., 160., 100., 174., 86.);

    pm.stroke_path(&left.finish().unwrap(), &faded, &stroke, t, None);
    pm.fill_path(&arrow_head((60., 64.), (-1., -1.), head_len, head_half), &faded, FillRule::Winding, t, None);
    pm.stroke_path(&stem.finish().unwrap(), &white, &stroke, t, None);
    pm.stroke_path(&right.finish().unwrap(), &white, &stroke, t, None);
    pm.fill_path(&arrow_head((196., 64.), (1., -1.), head_len, head_half), &white, FillRule::Winding, t, None);
    pm
}

fn main() {
    let pngs: Vec<(u32, Vec<u8>)> = SIZES.iter().map(|&s| (s, render(s).encode_png().unwrap())).collect();

    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(pngs.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * pngs.len() as u32;
    for (size, png) in &pngs {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        ico.extend_from_slice(&[dim, dim, 0, 0, 1, 0, 32, 0]);
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in &pngs {
        ico.extend_from_slice(png);
    }

    std::fs::create_dir_all("assets").unwrap();
    std::fs::write("assets/app.ico", ico).unwrap();
    std::fs::write("assets/app.png", &pngs.last().unwrap().1).unwrap();
    println!("wrote assets/app.ico and assets/app.png");
}
