//! Interactive picker: switch, add, edit and delete profiles.

use crate::git;
use crate::profile::{self, Profile, Store};
use anyhow::{Result, bail};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph};
use ratatui::{DefaultTerminal, Frame};

const LABELS: [&str; 5] = ["Id", "Name", "Email", "GitHub user", "SSH key"];
const LAST: usize = LABELS.len() - 1;

struct Form {
    /// Editing an existing profile: the id is fixed.
    editing: bool,
    fields: [String; 5],
    focus: usize,
    err: String,
}

impl Form {
    fn new() -> Self {
        Self {
            editing: false,
            fields: Default::default(),
            focus: 0,
            err: String::new(),
        }
    }

    fn from(id: &str, p: &Profile) -> Self {
        Self {
            editing: true,
            fields: [
                id.into(),
                p.name.clone(),
                p.email.clone(),
                p.github.clone().unwrap_or_default(),
                p.ssh_key.clone().unwrap_or_default(),
            ],
            focus: 1,
            err: String::new(),
        }
    }

    fn first(&self) -> usize {
        usize::from(self.editing)
    }

    fn next(&mut self) {
        self.focus = if self.focus == LAST {
            self.first()
        } else {
            self.focus + 1
        };
    }

    fn prev(&mut self) {
        self.focus = if self.focus == self.first() {
            LAST
        } else {
            self.focus - 1
        };
    }

    fn profile(&self) -> (String, Profile) {
        let f = &self.fields;
        let p = Profile {
            name: f[1].trim().into(),
            email: f[2].trim().into(),
            github: profile::opt(&f[3]),
            ssh_key: profile::opt(&f[4]),
        };
        (f[0].trim().into(), p)
    }
}

enum Mode {
    List,
    Form(Form),
    Delete(String),
}

struct App {
    store: Store,
    state: ListState,
    note: String,
    mode: Mode,
}

pub fn run() -> Result<()> {
    let mut app = App {
        store: profile::load()?,
        state: ListState::default(),
        note: String::new(),
        mode: Mode::List,
    };
    app.select_first();
    let mut term = ratatui::init();
    let res = app.event_loop(&mut term);
    ratatui::restore();
    res
}

impl App {
    fn ids(&self) -> Vec<String> {
        self.store.keys().cloned().collect()
    }

    fn selected(&self) -> Option<String> {
        self.ids().get(self.state.selected()?).cloned()
    }

    fn select_first(&mut self) {
        self.state
            .select(if self.store.is_empty() { None } else { Some(0) });
    }

    /// Reload from disk and keep the selection on `id` (or clamp it).
    fn reload(&mut self, id: Option<&str>) -> Result<()> {
        self.store = profile::load()?;
        let ids = self.ids();
        let keep = id.and_then(|id| ids.iter().position(|x| x == id));
        let clamp = self
            .state
            .selected()
            .map(|i| i.min(ids.len().saturating_sub(1)));
        self.state.select(if ids.is_empty() {
            None
        } else {
            Some(keep.or(clamp).unwrap_or(0))
        });
        Ok(())
    }

    fn event_loop(&mut self, term: &mut DefaultTerminal) -> Result<()> {
        loop {
            term.draw(|f| self.draw(f))?;
            let Event::Key(k) = event::read()? else {
                continue;
            };
            if k.kind == KeyEventKind::Press && self.key(k.code)? {
                return Ok(());
            }
        }
    }

    /// Handle one key. Returns true to quit.
    fn key(&mut self, code: KeyCode) -> Result<bool> {
        match std::mem::replace(&mut self.mode, Mode::List) {
            Mode::List => return self.key_list(code),
            Mode::Delete(id) => {
                if code == KeyCode::Char('y') {
                    self.store.remove(&id);
                    profile::save(&self.store)?;
                    self.note = format!("removed {id}");
                    self.reload(None)?;
                }
            }
            Mode::Form(form) => self.key_form(form, code),
        }
        Ok(false)
    }

    fn key_list(&mut self, code: KeyCode) -> Result<bool> {
        let n = self.store.len();
        let cur = self.state.selected().unwrap_or(0);
        match code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
            KeyCode::Down | KeyCode::Char('j') if n > 0 => self.state.select(Some((cur + 1) % n)),
            KeyCode::Up | KeyCode::Char('k') if n > 0 => self.state.select(Some((cur + n - 1) % n)),
            KeyCode::Enter | KeyCode::Char('g') => {
                if let Some(id) = self.selected() {
                    self.note = profile::apply(&id, code == KeyCode::Char('g'))
                        .unwrap_or_else(|e| format!("error: {e}"));
                }
            }
            KeyCode::Char('a') => self.mode = Mode::Form(Form::new()),
            KeyCode::Char('e') => {
                if let Some(id) = self.selected() {
                    self.mode = Mode::Form(Form::from(&id, &self.store[&id]));
                }
            }
            KeyCode::Char('d') => {
                if let Some(id) = self.selected() {
                    self.mode = Mode::Delete(id);
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn key_form(&mut self, mut form: Form, code: KeyCode) {
        let editable = !(form.editing && form.focus == 0);
        match code {
            KeyCode::Esc => return, // mode is already List
            KeyCode::Tab | KeyCode::Down => form.next(),
            KeyCode::BackTab | KeyCode::Up => form.prev(),
            KeyCode::Enter if form.focus < LAST => form.next(),
            KeyCode::Enter => match self.save(&form) {
                Ok(id) => {
                    self.note = format!("saved {id} (Enter on it to switch)");
                    let _ = self.reload(Some(&id));
                    return;
                }
                Err(e) => form.err = e.to_string(),
            },
            KeyCode::Backspace if editable => {
                form.fields[form.focus].pop();
            }
            KeyCode::Char(c) if editable => form.fields[form.focus].push(c),
            _ => {}
        }
        self.mode = Mode::Form(form);
    }

    fn save(&mut self, form: &Form) -> Result<String> {
        let (id, p) = form.profile();
        profile::validate(&id, &p)?;
        if !form.editing && self.store.contains_key(&id) {
            bail!("profile '{id}' already exists");
        }
        self.store.insert(id.clone(), p);
        profile::save(&self.store)?;
        Ok(id)
    }

    fn draw(&mut self, f: &mut Frame) {
        let name = git::git(&["config", "user.name"]).unwrap_or_default();
        let email = git::git(&["config", "user.email"]).unwrap_or_default();
        let active = git::git(&["config", "gas.profile"]);
        let scope = if git::in_repo() { "repo" } else { "global" };

        let [top, mid, bot] = Layout::vertical([
            Constraint::Length(4),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .areas(f.area());
        f.render_widget(
            Paragraph::new(format!(
                "{name} <{email}>\nprofile: {}  (scope: {scope})",
                active.as_deref().unwrap_or("none")
            ))
            .block(Block::bordered().title(" gas · current identity ")),
            top,
        );

        let block = Block::bordered().title(" profiles ");
        if self.store.is_empty() {
            f.render_widget(
                Paragraph::new("no profiles yet. press a to add one").block(block),
                mid,
            );
        } else {
            let items: Vec<ListItem> = self
                .store
                .iter()
                .map(|(id, p)| {
                    let mark = if active.as_deref() == Some(id.as_str()) {
                        "● "
                    } else {
                        "  "
                    };
                    ListItem::new(format!(
                        "{mark}{id}  {} <{}>  gh:{}",
                        p.name,
                        p.email,
                        p.github.as_deref().unwrap_or("-")
                    ))
                })
                .collect();
            let list = List::new(items)
                .block(block)
                .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
                .highlight_symbol("> ");
            f.render_stateful_widget(list, mid, &mut self.state);
        }

        let hint = match self.mode {
            Mode::List => {
                "↑/↓ select · enter switch · g global · a add · e edit · d delete · q quit"
            }
            Mode::Form(_) => "tab/↑/↓ move · enter next/save · esc cancel",
            Mode::Delete(_) => "y delete · any other key cancel",
        };
        f.render_widget(Paragraph::new(format!("{}\n{hint}", self.note).dim()), bot);

        match &self.mode {
            Mode::Form(form) => draw_form(f, form),
            Mode::Delete(id) => {
                let r = centered(f.area(), 44, 3);
                f.render_widget(Clear, r);
                f.render_widget(
                    Paragraph::new(format!("Delete '{id}'? (y/N)")).block(Block::bordered()),
                    r,
                );
            }
            Mode::List => {}
        }
    }
}

fn draw_form(f: &mut Frame, form: &Form) {
    let r = centered(f.area(), 64, 10);
    f.render_widget(Clear, r);
    let mut lines: Vec<String> = (0..LABELS.len())
        .map(|i| {
            let cursor = if i == form.focus { "▏" } else { "" };
            format!(
                "{} {:<12} {}{cursor}",
                if i == form.focus { ">" } else { " " },
                LABELS[i],
                form.fields[i]
            )
        })
        .collect();
    lines.push(String::new());
    lines.push(if form.err.is_empty() {
        "GitHub user and SSH key are optional".into()
    } else {
        format!("! {}", form.err)
    });
    let title = if form.editing {
        " edit profile "
    } else {
        " add profile "
    };
    f.render_widget(
        Paragraph::new(lines.join("\n")).block(Block::bordered().title(title)),
        r,
    );
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let (w, h) = (w.min(area.width), h.min(area.height));
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}
