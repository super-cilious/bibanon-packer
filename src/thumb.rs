use super::*;
use image::*;
use imageproc::*;
use imageproc::rect::Rect as IRect;
use hsl::HSL;
use rusttype::{Font, Scale, point, PositionedGlyph};
use rand::Rng;

//templating consts

pub type Rect = (u32, u32, u32, u32);
pub type Pos = (u32, u32);

pub const INFO_RECT: Rect = (7, 7, 692, 37);
pub const ICON_RECT: Rect = (15, 8, 43, 36);
pub const INFO_TEXT_RECT: Rect = (49, 8, 675, 36);
pub const IMAGE_RECT: Rect = (7, 37, 692, 774);
pub const IMAGE_OVERLAY_RECT: Rect = (25, 55, 222, 475);
pub const TITLE_RECT: Rect = (7, 816, 692, 992);
pub const TITLE_PADDING: u32 = 18;
pub const SUBTITLE: Pos = (35, 955);
pub const C64_CHARS: &str = "abcdefghijklmnopqrstuvwxyz"; //alpaahabeat
pub const GRID_SIZE: u32 = 5;

pub const HUE_INCR: i32 = 40;

pub const ASSETS: &str = "assets";

pub fn make_thumb(bg: Option<Vec<u8>>, meta: &Metadata) -> Res<Vec<u8>> {
    let assets = std::env::current_exe()?.parent().unwrap().to_path_buf().with(ASSETS);
    let regular = Font::from_bytes(include_bytes!("../assets/LibreBaskerville-Regular.ttf") as &[u8])?;
    let bold = Font::from_bytes(include_bytes!("../assets/LibreBaskerville-Bold.ttf") as &[u8])?;
    let symbols = Font::from_bytes(include_bytes!("../assets/rosette110621.ttf") as &[u8])?;
    let mut rng = rand::thread_rng();

    let overlay = load_from_memory_with_format(include_bytes!("../assets/template.png"), ImageFormat::PNG)?;
    let mut thumb = DynamicImage::new_rgb8(700, 1000);

    let h: i32 = (HUE_INCR * rng.gen_range(0, 20))%360;
    let h2: i32 = (h+180)%360; //rotate 180 and complement other color

    let (bg_w, bg_h) = (get_w(IMAGE_RECT), get_h(IMAGE_RECT));
    let mut bgimage = bg.ok_or(format_err!("No bg image found!"))
        .and_then(|bg| load_from_memory(&*bg).map_err(Error::from)).unwrap_or_else(|_| {

        let fg = HSL {h: h as f64, s: 100.0, l: 40.0}.to_rgb();
        let bg = HSL {h: h2 as f64, s: 50.0, l: 50.0}.to_rgb();
        let mut ply_bg = DynamicImage::new_rgb8(bg_w, bg_h);
        let incr = bg_w/GRID_SIZE;

        let char1 = rng.gen_range(0, C64_CHARS.len());
        let char2 = rng.gen_range(0, C64_CHARS.len());
        let chars: Vec<char> = C64_CHARS.chars().collect();
        let chars = [chars[char1], chars[char2]];

        draw_rect(&mut ply_bg, (0,0,bg_w,bg_h), bg);

        for x in 0..GRID_SIZE {
            for y in 0..GRID_SIZE {
                let char1or2 = rng.gen_range(0, 2);
                let chosen = chars[char1or2].to_string();
                draw_text(&mut ply_bg, Rgba([fg.0, fg.1, fg.2, 255]), (x*incr, y*incr), Scale::uniform(incr as f32), &symbols, &chosen)
            }
        }

        ply_bg
    });

    bgimage = bgimage.resize_to_fill(bg_w, bg_h, imageops::FilterType::Gaussian);
    bgimage.adjust_contrast(4.0);
    imageops::overlay(&mut thumb, &bgimage, IMAGE_RECT.0, IMAGE_RECT.1);

    let hsl = HSL {h: h as f64, s: 70.0, l: 35.0};

    let bottom_rgb = HSL::to_rgb(&hsl);
    draw_rect(&mut thumb, TITLE_RECT, bottom_rgb);

    draw_text_box(&mut thumb, Rgba([255,255,255,255]), pad(TITLE_RECT, TITLE_PADDING), 10, Scale::uniform(50.0), &bold, &meta.title);

    if let Some(x) = &meta.sub {
        draw_text(&mut thumb, Rgba([255, 255, 255, 255]), SUBTITLE, Scale::uniform(30.0), &bold, x);
    }

    let hsl2 = HSL {h: h2 as f64, s: 35.0, l: 30.0};

    let top_rgb = HSL::to_rgb(&hsl2);
    draw_rect(&mut thumb, INFO_RECT, top_rgb);
    draw_text(&mut thumb, Rgba([0,0,0,255]), (INFO_TEXT_RECT.0, INFO_TEXT_RECT.1), Scale::uniform(25.0), &regular, &meta.stats.join(" • "));

    if let Ok(source) = fs::read(assets.with(&meta.source).ext("png")) {
        let mut img = load_from_memory_with_format(&*source, ImageFormat::PNG)?;
        img = img.resize_to_fill(get_w(ICON_RECT), get_h(ICON_RECT), imageops::FilterType::Gaussian);
        imageops::overlay(&mut thumb, &img, ICON_RECT.0, ICON_RECT.1);
    }

    if let Ok(type_) = fs::read(assets.with (&meta.type_).ext("png")) {
        let mut img = load_from_memory_with_format(&*type_, ImageFormat::PNG)?;
        img = img.resize_to_fill(get_w(IMAGE_OVERLAY_RECT), get_h(IMAGE_OVERLAY_RECT), imageops::FilterType::Nearest); //for crisp pixel art
        transparentize(&mut img, 0.3);
        imageops::overlay(&mut thumb, &img, IMAGE_OVERLAY_RECT.0, IMAGE_OVERLAY_RECT.1);
    }

    imageops::overlay(&mut thumb, &overlay, 0, 0);

    let mut buf = Vec::new();
    thumb.write_to(&mut buf, ImageFormat::JPEG)?;

    Ok(buf)
}


fn get_w(rect: Rect) -> u32 {
    rect.2 - rect.0
}

fn get_h(rect: Rect) -> u32 {
    rect.3 - rect.1
}

fn pad(rect: Rect, amnt: u32) -> Rect {
    (rect.0 + amnt, rect.1 + amnt, rect.2 - amnt, rect.3 - amnt)
}

fn transparentize(img: &mut DynamicImage, perc: f32) {
    let dim = img.dimensions();
    for x in 0..dim.0 {
        for y in 0..dim.1 {
            let p = img.get_pixel(x, y).map_with_alpha(|rgb| rgb, |alpha| (((alpha as f32/225.0)/perc)*225.0) as u8);
            img.put_pixel(x,y, p);
        }
    }
}

fn draw_rect(img: &mut DynamicImage, pos: Rect, rgb: (u8, u8, u8)) {
    drawing::draw_filled_rect_mut(img, IRect::at(pos.0 as i32, pos.1 as i32).of_size(get_w(pos), get_h(pos)), Rgba([rgb.0, rgb.1, rgb.2, 1]))
}

fn draw_glyph(image: &mut DynamicImage, color: Rgba<u8>, alpha: f32, gv: f32, image_x: i32, image_y: i32) {
    let pixel = image.get_pixel(image_x as u32, image_y as u32);
    let weighted_color = pixelops::weighted_sum(pixel, color, 1.0-(alpha*gv), alpha*gv);
    image.put_pixel(image_x as u32, image_y as u32, weighted_color);
}

//slightly modified versions
fn draw_text_box(
    image: &mut DynamicImage,
    color: Rgba<u8>,
    rect: Rect, padding: i32,
    scale: Scale,
    font: &Font,
    text: &str,
) {
    let alpha = color.data[3] as f32/255.0;
    let v_metrics = font.v_metrics(scale);
    let offset = point(0.0, v_metrics.ascent);
    let rectwidth = get_w(rect) as i32;

    let mut lines = Vec::new();
    let split_txt = text.replace(' ', "   ");

    for txt in split_txt.split("  ") {
        let txtglyphs: Vec<PositionedGlyph<'_>> = font.layout(txt, scale, offset).collect();
        let width: i32 = txtglyphs.iter().last().and_then(|x| x.pixel_bounding_box().map(|x| x.max.x)).unwrap_or(0);

        if width > rectwidth {
            lines.push((width, vec![txtglyphs]));
        } else {
            if let Some(x) = lines.last_mut() {
                x.0 += width;
                x.1.push(txtglyphs);
            } else {
                lines.push((width, vec![txtglyphs]));
            }
        }
    }

    let mut newln = 0;
    for line in lines.iter() {
        let mut cnewln = 0;
        let mut hoffset = 0;

        let off = (rectwidth - line.0.min(rectwidth))/2;

        for segment in &line.1 {
            let start = hoffset;

            for g in segment {
                if let Some(bb) = g.pixel_bounding_box() {
                    cnewln = bb.height();
                    hoffset = bb.max.x;

                    let hoffset = off + start;
                    let voffset = newln;

                    g.draw(|gx, gy, gv| {
                        let gx = gx as i32 + bb.min.x;
                        let gy = gy as i32 + bb.min.y;

                        let image_x = gx + hoffset + rect.0 as i32;
                        let image_y = gy + voffset + rect.1 as i32;

                        if image_x >= 0 && image_x < rect.2 as i32 && image_y >= 0 && image_y < rect.3 as i32 {
                            draw_glyph(image, color, alpha, gv, image_x, image_y);
                        }
                    });
                }
            }
        }

        cnewln += padding;
        newln += cnewln;
    }
}

fn draw_text(
    image: &mut DynamicImage,
    color: Rgba<u8>,
    pos: Pos,
    scale: Scale,
    font: &Font,
    text: &str,
) {
    let alpha = color.data[3] as f32/255.0;
    let v_metrics = font.v_metrics(scale);
    let offset = point(0.0, v_metrics.ascent);

    let glyphs: Vec<PositionedGlyph<'_>> = font.layout(text, scale, offset).collect();

    for g in glyphs {
        if let Some(bb) = g.pixel_bounding_box() {
            g.draw(|gx, gy, gv| {
                let gx = gx as i32 + bb.min.x;
                let gy = gy as i32 + bb.min.y;

                let image_x = gx + pos.0 as i32;
                let image_y = gy + pos.1 as i32;

                let image_width = image.width() as i32;
                let image_height = image.height() as i32;

                if image_x >= 0 && image_x < image_width && image_y >= 0 && image_y < image_height {
                    draw_glyph(image, color, alpha, gv, image_x, image_y);
                }
            })
        }
    }
}