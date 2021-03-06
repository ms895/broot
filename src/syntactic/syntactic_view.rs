use {
    super::Syntaxer,
    crate::{
        command::{ScrollCommand},
        display::{CropWriter, LONG_SPACE, Screen, W},
        errors::ProgramError,
        pattern::{NameMatch, Pattern},
        skin::PanelSkin,
    },
    crossterm::{
        cursor,
        style::{Color, Print, SetBackgroundColor, SetForegroundColor},
        QueueableCommand,
    },
    std::{
        io::{self, BufRead},
        path::{Path, PathBuf},
    },
    syntect::{
        highlighting::Style,
        easy::HighlightFile,
    },
    termimad::Area,
};

#[derive(Debug)]
pub struct SyntacticRegion {
    pub fg: Color,
    pub string: String,
}

/// number of lines initially parsed (and shown before scroll).
/// It's best to have this greater than the screen height to
/// avoid two initial parsings.
const INITIAL_HEIGHT: usize = 150;

impl SyntacticRegion {
    pub fn from_syntect(region: &(Style, &str)) -> Self {
        let fg = Color::Rgb {
            r: region.0.foreground.r,
            g: region.0.foreground.g,
            b: region.0.foreground.b,
        };
        let string = str::replace(region.1, '\t', "    ");
        Self { fg, string }
    }
}

pub struct SyntacticLine {
    pub number: usize, // starting at 1
    pub contents: Vec<SyntacticRegion>,
    pub name_match: Option<NameMatch>,
}

pub struct SyntacticView {
    path: PathBuf,
    pattern: Pattern,
    lines: Vec<SyntacticLine>,
    content_height: usize, // bigger than lines.len() if not fully loaded
    scroll: i32,
    page_height: i32,
}

impl SyntacticView {

    pub fn new(
        path: &Path,
        pattern: Pattern,
    ) -> io::Result<Self> {
        let mut sv = Self {
            path: path.to_path_buf(),
            pattern,
            lines: Vec::new(),
            content_height: 0,
            scroll: 0,
            page_height: 0,
        };
        sv.prepare(false)?;
        Ok(sv)
    }

    /// try to load and parse the content or part of it.
    /// Check the correct encoding of the whole file
    ///  even when `full` is false.
    fn prepare(&mut self, full: bool) -> io::Result<()> {
        lazy_static! {
            static ref SYNTAXER: Syntaxer = Syntaxer::new();
        }
        //let theme_key = "base16-ocean.dark";
        //let theme_key = "Solarized (dark)";
        //let theme_key = "base16-eighties.dark";
        let theme_key = "base16-mocha.dark";
        let theme = match SYNTAXER.theme_set.themes.get(theme_key) {
            Some(theme) => theme,
            None => {
                warn!("theme not found : {:?}", theme_key);
                SYNTAXER.theme_set.themes.iter().next().unwrap().1
            }
        };
        let syntax_set = &SYNTAXER.syntax_set;
        let mut highlighter = HighlightFile::new(&self.path, syntax_set, theme)?;
        self.lines.clear();
        self.content_height = 0;
        let mut number = 0;
        for line in highlighter.reader.lines() {
            let line = line?;
            number += 1;
            if self.pattern.is_some() && self.pattern.score_of_string(&line).is_none() {
                continue;
            }
            self.content_height += 1;
            if full || self.lines.len() < INITIAL_HEIGHT {
                let name_match = self.pattern.search_string(&line);
                let contents = highlighter.highlight_lines
                    .highlight(&line, &syntax_set)
                    .iter()
                    .map(|r| SyntacticRegion::from_syntect(r))
                    .collect();
                self.lines.push(SyntacticLine {
                    contents,
                    name_match,
                    number,
                });
            }
        }
        Ok(())
    }

    pub fn is_fully_loaded(&self) -> bool {
        self.content_height == self.lines.len()
    }

    pub fn try_scroll(
        &mut self,
        cmd: ScrollCommand,
    ) -> bool {
        let old_scroll = self.scroll;
        self.scroll = (self.scroll + cmd.to_lines(self.page_height))
            .min(self.content_height as i32 - self.page_height + 1)
            .max(0);
        self.scroll != old_scroll
    }

    pub fn display(
        &mut self,
        w: &mut W,
        _screen: &Screen,
        panel_skin: &PanelSkin,
        area: &Area,
    ) -> Result<(), ProgramError> {
        self.page_height = area.height as i32;
        if !self.is_fully_loaded() && self.scroll + self.page_height > self.lines.len() as i32 {
            debug!("now fully loading");
            self.prepare(true)?;
        }
        let max_number_len = self.lines.last().map_or(0, |l|l.number).to_string().len();
        let show_line_number = area.width > 55 || ( self.pattern.is_some() && area.width > 8 );
        let line_count = area.height as usize;
        let styles = &panel_skin.styles;
        let bg = styles.preview.get_bg().or(styles.default.get_bg()).unwrap_or(Color::AnsiValue(238));
        let match_bg = styles.preview_match.get_bg().unwrap_or(Color::AnsiValue(28));
        let code_width = area.width as usize - 1; // 1 char left for scrollbar
        let scrollbar = area.scrollbar(self.scroll, self.content_height as i32);
        let scrollbar_fg = styles.scrollbar_thumb.get_fg()
            .or(styles.preview.get_fg())
            .unwrap_or_else(|| Color::White);
        for y in 0..line_count {
            w.queue(cursor::MoveTo(area.left, y as u16 + area.top))?;
            let mut cw = CropWriter::new(w, code_width);
            if let Some(line) = self.lines.get(self.scroll as usize + y) {
                if show_line_number {
                    cw.queue_g_string(
                        &styles.preview_line_number,
                        format!(" {:w$} ", line.number, w = max_number_len),
                    )?;
                    cw.w.queue(SetBackgroundColor(bg))?;
                } else {
                    cw.w.queue(SetBackgroundColor(bg))?;
                    cw.queue_unstyled_str(" ")?;
                }
                if let Some(nm) = &line.name_match {
                    let mut dec = 0;
                    let mut nci = 0; // next char index
                    for content in &line.contents {
                        let s = &content.string;
                        let pos = &nm.pos;
                        let mut x = 0;
                        cw.w.queue(SetForegroundColor(content.fg))?;
                        while nci < pos.len() && pos[nci]>=dec && pos[nci]<dec+s.len() {
                            let i = pos[nci]-dec;
                            if i > x {
                                cw.queue_unstyled_str(&s[x..i])?;
                            }
                            cw.w.queue(SetBackgroundColor(match_bg))?;
                            cw.queue_unstyled_str(&s[i..i+1])?;
                            cw.w.queue(SetBackgroundColor(bg))?;
                            nci += 1;
                            x = i+1;
                        }
                        if x < s.len() {
                            cw.queue_unstyled_str(&s[x..])?;
                        }
                        dec += content.string.len();
                    }
                } else {
                    for content in &line.contents {
                        cw.w.queue(SetForegroundColor(content.fg))?;
                        cw.queue_unstyled_str(&content.string)?;
                    }
                }
            }
            cw.fill(&styles.preview, LONG_SPACE)?;
            w.queue(SetBackgroundColor(bg))?;
            if is_thumb(y, scrollbar) {
                w.queue(SetForegroundColor(scrollbar_fg))?;
                w.queue(Print('▐'))?;
            } else {
                w.queue(Print(' '))?;
            }
        }
        Ok(())
    }
}

fn is_thumb(y: usize, scrollbar: Option<(u16, u16)>) -> bool {
    if let Some((sctop, scbottom)) = scrollbar {
        let y = y as u16;
        if sctop <= y && y <= scbottom {
            return true;
        }
    }
    false
}

