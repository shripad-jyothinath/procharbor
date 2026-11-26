use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Stroke, Vec2,
};
use egui_extras::{Column, TableBuilder};

use crate::charts::{
    ChartSeries, draw_chart_area, draw_composition_bar, draw_sparkline, draw_thin_bar,
};
use crate::desktop::{DesktopIndex, ProcessCategory};
use crate::fmt::{
    fmt_bytes, fmt_clock, fmt_cpu_time, fmt_pct, fmt_rate, fmt_thousands, fmt_uptime, nice_label,
};
use crate::icons::IconCache;
use crate::sampler::{
    AppHistoryRow, ProcessInfo, Sampler, ServiceRow, Snapshot, StartupRow, UserRow, kill_pids,
    terminate_pids,
};
use crate::theme::{Palette, ThemeMode, apply_egui_style, load_color};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tab {
    Processes,
    Performance,
    Apps,
    Startup,
    Users,
    Details,
    Services,
}

impl Tab {
    const ALL: [Tab; 7] = [
        Tab::Processes,
        Tab::Performance,
        Tab::Apps,
        Tab::Startup,
        Tab::Users,
        Tab::Details,
        Tab::Services,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::Processes => "Processes",
            Tab::Performance => "Performance",
            Tab::Apps => "App history",
            Tab::Startup => "Startup",
            Tab::Users => "Users",
            Tab::Details => "Details",
            Tab::Services => "Services",
        }
    }

    fn search_placeholder(self) -> &'static str {
        match self {
            Tab::Processes => "Search processes…",
            Tab::Apps => "Search apps…",
            Tab::Startup => "Search startup items…",
            Tab::Users => "Search users…",
            Tab::Details => "Search processes…",
            Tab::Services => "Search services…",
            Tab::Performance => "Search…",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PerfItem {
    Cpu,
    Memory,
    Disk,
    Wifi,
    Gpu,
}

impl PerfItem {
    const ALL: [PerfItem; 5] = [
        PerfItem::Cpu,
        PerfItem::Memory,
        PerfItem::Disk,
        PerfItem::Wifi,
        PerfItem::Gpu,
    ];

    fn label(self) -> &'static str {
        match self {
            PerfItem::Cpu => "CPU",
            PerfItem::Memory => "Memory",
            PerfItem::Disk => "Disk",
            PerfItem::Wifi => "Wi-Fi",
            PerfItem::Gpu => "GPU",
        }
    }

    fn color(self) -> Color32 {
        match self {
            PerfItem::Cpu => Color32::from_rgb(0x4c, 0xc2, 0xff),
            PerfItem::Memory => Color32::from_rgb(0xa6, 0x8b, 0xff),
            PerfItem::Disk => Color32::from_rgb(0x5a, 0xd4, 0x8a),
            PerfItem::Wifi => Color32::from_rgb(0xf7, 0xb9, 0x55),
            PerfItem::Gpu => Color32::from_rgb(0xff, 0x76, 0x76),
        }
    }
}

#[derive(Clone, Copy)]
struct SortState {
    key: &'static str,
    asc: bool,
}

#[derive(Clone)]
enum SelectedTarget {
    Group {
        name: String,
        category: ProcessCategory,
        pid: u32,
        procs: Vec<ProcessInfo>,
    },
    Process {
        pid: u32,
        name: String,
    },
    Startup {
        name: String,
        unit_or_file: String,
        status: String,
        entry_type: String,
    },
}

struct ProcessGroup {
    name: String,
    category: ProcessCategory,
    procs: Vec<ProcessInfo>,
    cpu: f64,
    mem: u64,
    disk: u64,
    net: u64,
    gpu: f64,
    top_pid: u32,
    exe: String,
    cmdline: String,
    status: String,
}

enum ProcRow {
    Header { label: &'static str, status: String },
    Group { group_idx: usize, open: bool },
    Child { group_idx: usize, proc_idx: usize },
}

#[derive(Clone, Copy, PartialEq)]
enum KillMode {
    End,
    Kill,
}

struct KillConfirmDialog {
    title: String,
    detail: String,
    pids: Vec<u32>,
    mode: KillMode,
}

pub struct ProcHarborApp {
    sampler: Arc<Sampler>,
    snap: Snapshot,
    theme: ThemeMode,
    current_tab: Tab,
    perf_item: PerfItem,
    paused: bool,
    search_text: String,
    focus_search: bool,
    expanded_groups: HashSet<(String, ProcessCategory)>,
    selected: Option<SelectedTarget>,

    sorts: HashMap<Tab, SortState>,

    icons: IconCache,
    desktop: DesktopIndex,

    services_state: Arc<Mutex<(Option<Vec<ServiceRow>>, f64, bool)>>,
    startup_state: Arc<Mutex<(Option<Vec<StartupRow>>, f64, bool)>>,

    confirm_dialog: Option<KillConfirmDialog>,
    error_dialog: Option<String>,
    last_seq: u64,
}

impl ProcHarborApp {
    pub fn new(cc: &eframe::CreationContext<'_>, sampler: Arc<Sampler>) -> Self {
        let theme = ThemeMode::Dark;
        apply_egui_style(&cc.egui_ctx, theme);

        let icons = IconCache::new();
        let desktop = DesktopIndex::build(|name| icons.has_icon(name));
        let last_seq = sampler.seq();
        let snap = sampler.snapshot();

        let mut sorts = HashMap::new();
        sorts.insert(
            Tab::Processes,
            SortState {
                key: "cpu",
                asc: false,
            },
        );
        sorts.insert(
            Tab::Details,
            SortState {
                key: "pid",
                asc: true,
            },
        );
        sorts.insert(
            Tab::Apps,
            SortState {
                key: "cpu_s",
                asc: false,
            },
        );
        sorts.insert(
            Tab::Users,
            SortState {
                key: "cpu",
                asc: false,
            },
        );
        sorts.insert(
            Tab::Startup,
            SortState {
                key: "type",
                asc: true,
            },
        );
        sorts.insert(
            Tab::Services,
            SortState {
                key: "unit",
                asc: true,
            },
        );

        Self {
            sampler,
            snap,
            theme,
            current_tab: Tab::Processes,
            perf_item: PerfItem::Cpu,
            paused: false,
            search_text: String::new(),
            focus_search: false,
            expanded_groups: HashSet::new(),
            selected: None,
            sorts,
            icons,
            desktop,
            services_state: Arc::new(Mutex::new((None, 0.0, false))),
            startup_state: Arc::new(Mutex::new((None, 0.0, false))),
            confirm_dialog: None,
            error_dialog: None,
            last_seq,
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let ctrl_f = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F));
        if ctrl_f {
            if self.current_tab == Tab::Performance {
                self.current_tab = Tab::Processes;
                self.selected = None;
            }
            self.focus_search = true;
        }

        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if esc {
            if self.error_dialog.is_some() {
                self.error_dialog = None;
            } else if self.confirm_dialog.is_some() {
                self.confirm_dialog = None;
            } else if !self.search_text.is_empty() {
                self.search_text.clear();
            }
        }
    }

    fn refresh_snapshot_if_needed(&mut self) {
        let seq = self.sampler.seq();
        if !self.paused && seq != self.last_seq {
            self.snap = self.sampler.snapshot();
            self.last_seq = seq;
        }
        if self.current_tab == Tab::Services {
            self.ensure_services();
        } else if self.current_tab == Tab::Startup {
            self.ensure_startup(false);
        }
    }

    fn ensure_services(&self) {
        let now = crate::sampler::now_secs();
        let mut st = self.services_state.lock().unwrap();
        if st.2 {
            return;
        }
        if st.0.is_some() && now - st.1 < 10.0 {
            return;
        }
        st.2 = true;
        drop(st);

        let sampler = Arc::clone(&self.sampler);
        let state = Arc::clone(&self.services_state);
        thread::spawn(move || {
            let rows = sampler.services();
            let mut st = state.lock().unwrap();
            st.0 = Some(rows);
            st.1 = crate::sampler::now_secs();
            st.2 = false;
        });
    }

    fn ensure_startup(&self, force: bool) {
        let now = crate::sampler::now_secs();
        let mut st = self.startup_state.lock().unwrap();
        if st.2 {
            return;
        }
        if !force && st.0.is_some() && now - st.1 < 30.0 {
            return;
        }
        st.2 = true;
        drop(st);

        let sampler = Arc::clone(&self.sampler);
        let state = Arc::clone(&self.startup_state);
        thread::spawn(move || {
            let rows = sampler.startup();
            let mut st = state.lock().unwrap();
            st.0 = Some(rows);
            st.1 = crate::sampler::now_secs();
            st.2 = false;
        });
    }

    fn toggle_sort(&mut self, tab: Tab, key: &'static str, is_text: bool) {
        let cur = self
            .sorts
            .get(&tab)
            .copied()
            .unwrap_or(SortState { key, asc: is_text });
        let next = if cur.key == key {
            SortState { key, asc: !cur.asc }
        } else {
            SortState { key, asc: is_text }
        };
        self.sorts.insert(tab, next);
    }

    fn sort_header_button(
        &mut self,
        ui: &mut egui::Ui,
        tab: Tab,
        title: &str,
        key: &'static str,
        is_text: bool,
        right_align: bool,
    ) {
        let pal = self.theme.palette();
        let cur = self.sorts.get(&tab).copied();
        let indicator = match cur {
            Some(s) if s.key == key => {
                if s.asc {
                    " ▲"
                } else {
                    " ▼"
                }
            }
            _ => "",
        };
        let text = RichText::new(format!("{}{}", title, indicator))
            .size(12.0)
            .color(if indicator.is_empty() {
                pal.dim
            } else {
                pal.text
            });

        let layout = if right_align {
            Layout::right_to_left(Align::Center)
        } else {
            Layout::left_to_right(Align::Center)
        };
        ui.with_layout(layout, |ui| {
            let resp = ui.add(egui::Button::new(text).frame(false));
            if resp.clicked() {
                self.toggle_sort(tab, key, is_text);
            }
        });
    }

    fn open_kill_dialog(&mut self, mode: KillMode) {
        let Some(ref sel) = self.selected else {
            return;
        };
        let (verb, signal, warning) = match mode {
            KillMode::End => ("End", "SIGTERM", ""),
            KillMode::Kill => ("Kill", "SIGKILL", " immediately. Unsaved work will be lost"),
        };
        match sel {
            SelectedTarget::Group {
                name, pid, procs, ..
            } => {
                let pids: Vec<u32> = procs.iter().map(|p| p.pid).collect();
                let count = pids.len();
                let (title, detail) = if count > 1 {
                    (
                        format!("{} \"{}\" ({} processes)?", verb, name, count),
                        format!(
                            "All {} processes will receive {}{}.",
                            count, signal, warning
                        ),
                    )
                } else {
                    (
                        format!("{} \"{}\"?", verb, name),
                        format!("PID {} will receive {}{}.", pid, signal, warning),
                    )
                };
                self.confirm_dialog = Some(KillConfirmDialog {
                    title,
                    detail,
                    pids: if pids.is_empty() { vec![*pid] } else { pids },
                    mode,
                });
            }
            SelectedTarget::Process { pid, name } => {
                self.confirm_dialog = Some(KillConfirmDialog {
                    title: format!("{} \"{}\"?", verb, name),
                    detail: format!("PID {} will receive {}{}.", pid, signal, warning),
                    pids: vec![*pid],
                    mode,
                });
            }
            SelectedTarget::Startup { .. } => {}
        }
    }

    fn render_header(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let pal = self.theme.palette();
        let frame = Frame::NONE
            .fill(pal.side)
            .inner_margin(Margin::symmetric(12, 8))
            .stroke(Stroke::new(1.0, pal.border_soft));

        egui::Panel::top("header").frame(frame).show(ui, |ui| {
            ui.horizontal(|ui| {
                if self.current_tab != Tab::Performance {
                    let search_id = ui.make_persistent_id("search_entry");
                    let resp = ui.add_sized(
                        [340.0, 28.0],
                        egui::TextEdit::singleline(&mut self.search_text)
                            .id(search_id)
                            .hint_text(self.current_tab.search_placeholder()),
                    );
                    if self.focus_search {
                        resp.request_focus();
                        if let Some(mut state) = egui::TextEdit::load_state(&ctx, search_id) {
                            let len = self.search_text.chars().count();
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(len),
                                )));
                            state.store(&ctx, search_id);
                        }
                        self.focus_search = false;
                    }
                    if !self.search_text.is_empty() && ui.small_button("✕").clicked() {
                        self.search_text.clear();
                    }
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if matches!(self.current_tab, Tab::Processes | Tab::Details) {
                        let can_end = self.selected.is_some();
                        let end_text = RichText::new("End task").color(if can_end {
                            pal.danger
                        } else {
                            pal.faint
                        });
                        if ui
                            .add_enabled(can_end, egui::Button::new(end_text))
                            .on_hover_text("Ask the process to exit (SIGTERM)")
                            .clicked()
                        {
                            self.open_kill_dialog(KillMode::End);
                        }
                        let kill_text = RichText::new("Kill task").color(if can_end {
                            pal.danger
                        } else {
                            pal.faint
                        });
                        if ui
                            .add_enabled(can_end, egui::Button::new(kill_text))
                            .on_hover_text("Force kill the process (SIGKILL)")
                            .clicked()
                        {
                            self.open_kill_dialog(KillMode::Kill);
                        }
                    }

                    if self.current_tab == Tab::Startup {
                        if ui.button("↻ Refresh").clicked() {
                            self.ensure_startup(true);
                        }
                        if let Some(SelectedTarget::Startup {
                            name,
                            unit_or_file,
                            status,
                            entry_type,
                        }) = &self.selected.clone()
                        {
                            let is_enabled = status.eq_ignore_ascii_case("enabled");
                            let (lbl, col) = if is_enabled {
                                ("Disable", pal.danger)
                            } else {
                                ("Enable", pal.accent)
                            };
                            if ui.button(RichText::new(lbl).color(col)).clicked() {
                                let row = crate::sampler::StartupRow {
                                    name: name.clone(),
                                    unit_or_file: unit_or_file.clone(),
                                    status: status.clone(),
                                    entry_type: entry_type.clone(),
                                    ..Default::default()
                                };
                                match self.sampler.toggle_startup(&row) {
                                    Ok(new_state) => {
                                        let new_status =
                                            if new_state { "Enabled" } else { "Disabled" };
                                        self.selected = Some(SelectedTarget::Startup {
                                            name: name.clone(),
                                            unit_or_file: unit_or_file.clone(),
                                            status: new_status.to_string(),
                                            entry_type: entry_type.clone(),
                                        });
                                        self.ensure_startup(true);
                                    }
                                    Err(err) => {
                                        self.error_dialog = Some(err);
                                    }
                                }
                            }
                        }
                    }

                    let pause_lbl = if self.paused { "Resume" } else { "Pause" };
                    if ui.button(pause_lbl).clicked() {
                        self.paused = !self.paused;
                        if !self.paused {
                            self.last_seq = self.sampler.seq();
                            self.snap = self.sampler.snapshot();
                        }
                    }
                });
            });
        });
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let pal = self.theme.palette();
        let frame = Frame::NONE
            .fill(pal.side)
            .inner_margin(Margin::symmetric(10, 12))
            .stroke(Stroke::new(1.0, pal.border_soft));

        egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(236.0)
            .frame(frame)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    if let Some(tex) = self.icons.logo(&ctx) {
                        ui.image((tex.id(), Vec2::splat(24.0)));
                    }
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("ProcHarbor")
                            .size(15.5)
                            .strong()
                            .color(pal.text),
                    );
                });
                ui.add_space(10.0);

                for tab in Tab::ALL {
                    let selected = self.current_tab == tab;
                    let fill = if selected {
                        pal.accent_soft
                    } else {
                        Color32::TRANSPARENT
                    };
                    let resp = Frame::NONE
                        .fill(fill)
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(10, 7))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let dot_col = if selected { pal.accent } else { pal.dim };
                                let (dot_r, _) = ui.allocate_exact_size(
                                    Vec2::new(6.0, 14.0),
                                    egui::Sense::hover(),
                                );
                                if selected {
                                    ui.painter()
                                        .rect_filled(dot_r, CornerRadius::same(3), dot_col);
                                }
                                ui.add_space(4.0);
                                ui.label(RichText::new(tab.label()).size(13.5).color(pal.text));
                            });
                        })
                        .response
                        .interact(egui::Sense::click());

                    if resp.hovered() && !selected {
                        ui.painter().rect_stroke(
                            resp.rect,
                            CornerRadius::same(6),
                            Stroke::new(1.0, pal.border_soft),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if resp.clicked() && self.current_tab != tab {
                        self.current_tab = tab;
                        self.selected = None;
                    }
                }

                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    let (dot_col, status_txt) = if self.paused {
                        (pal.amber, "Paused")
                    } else if self.snap.system.is_some() {
                        (pal.green, "Live · 1s")
                    } else {
                        (pal.faint, "Starting…")
                    };

                    ui.horizontal(|ui| {
                        ui.add_space(6.0);
                        ui.label(RichText::new("●").size(11.0).color(dot_col));
                        ui.label(RichText::new(status_txt).size(12.0).color(pal.dim));
                    });
                    ui.add_space(4.0);

                    let theme_label = match self.theme {
                        ThemeMode::Dark => "Light theme",
                        ThemeMode::Light => "Dark theme",
                    };
                    if ui
                        .add(
                            egui::Button::new(RichText::new(theme_label).size(12.5).color(pal.dim))
                                .frame(false),
                        )
                        .clicked()
                    {
                        self.theme = match self.theme {
                            ThemeMode::Dark => ThemeMode::Light,
                            ThemeMode::Light => ThemeMode::Dark,
                        };
                        apply_egui_style(&ctx, self.theme);
                    }
                });
            });
    }

    fn render_summary(&self, ui: &mut egui::Ui) {
        let pal = self.theme.palette();
        let Some(ref sys) = self.snap.system else {
            return;
        };
        let totals = &self.snap.totals;

        Frame::NONE
            .fill(pal.bg)
            .inner_margin(Margin::symmetric(14, 7))
            .stroke(Stroke::new(1.0, pal.border_soft))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    summary_pair(ui, pal, "Processes", &totals.processes.to_string());
                    summary_pair(ui, pal, "Threads", &totals.threads.to_string());
                    summary_pair(ui, pal, "Handles", &totals.handles.to_string());
                    summary_pair(ui, pal, "CPU", &fmt_pct(Some(sys.cpu.util)));
                    summary_pair(ui, pal, "Memory", &fmt_pct(Some(sys.memory.percent)));
                    summary_pair(ui, pal, "Uptime", &fmt_uptime(sys.uptime));
                });
            });
    }

    fn filtered_procs(&self) -> Vec<ProcessInfo> {
        let q = self.search_text.trim().to_lowercase();
        if q.is_empty() {
            return self.snap.processes.clone();
        }
        self.snap
            .processes
            .iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&q)
                    || p.app.to_lowercase().contains(&q)
                    || p.pid.to_string().contains(&q)
                    || p.user.to_lowercase().contains(&q)
                    || p.cmdline.to_lowercase().contains(&q)
            })
            .cloned()
            .collect()
    }

    fn build_groups(&mut self, procs: &[ProcessInfo]) -> Vec<ProcessGroup> {
        let total_mem = self
            .snap
            .system
            .as_ref()
            .map(|s| s.memory.total)
            .unwrap_or(0);

        let mut user_uids: HashSet<u32> = self.snap.users.iter().map(|u| u.uid).collect();
        if user_uids.is_empty() {
            user_uids.insert(crate::sampler::my_uid());
        }

        let mut map: HashMap<(String, ProcessCategory), ProcessGroup> = HashMap::new();
        for p in procs {
            let cat = self
                .desktop
                .process_category(p.uid, &p.app, &p.exe, &p.cmdline, &user_uids);
            let key = (p.app.clone(), cat);
            let g = map.entry(key).or_insert_with(|| ProcessGroup {
                name: p.app.clone(),
                category: cat,
                procs: Vec::new(),
                cpu: 0.0,
                mem: 0,
                disk: 0,
                net: 0,
                gpu: 0.0,
                top_pid: p.pid,
                exe: String::new(),
                cmdline: String::new(),
                status: String::new(),
            });
            g.cpu += p.cpu;
            g.mem = g.mem.saturating_add(p.mem);
            g.disk = g.disk.saturating_add(p.disk_r).saturating_add(p.disk_w);
            g.net = g.net.saturating_add(p.net_up).saturating_add(p.net_down);
            g.gpu += p.gpu;
            g.procs.push(p.clone());
        }

        let mut groups: Vec<ProcessGroup> = map
            .into_values()
            .map(|mut g| {
                g.cpu = ((g.cpu * 10.0).round() / 10.0).min(100.0);
                g.gpu = ((g.gpu * 10.0).round() / 10.0).min(100.0);
                if total_mem > 0 {
                    g.mem = g.mem.min(total_mem);
                }
                g.procs
                    .sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then_with(|| a.pid.cmp(&b.pid)));
                if let Some(top) = g.procs.first() {
                    g.top_pid = top.pid;
                    g.exe = top.exe.clone();
                    g.cmdline = top.cmdline.clone();
                    g.status = if g.procs.len() > 1 {
                        format!("{} processes", g.procs.len())
                    } else {
                        top.status.clone()
                    };
                }
                g
            })
            .collect();

        let sort = self
            .sorts
            .get(&Tab::Processes)
            .copied()
            .unwrap_or(SortState {
                key: "cpu",
                asc: false,
            });
        groups.sort_by(|a, b| {
            let ord = match sort.key {
                "name" => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                "status" => a.status.to_lowercase().cmp(&b.status.to_lowercase()),
                "mem" => a.mem.cmp(&b.mem),
                "disk" => a.disk.cmp(&b.disk),
                "net" => a.net.cmp(&b.net),
                "gpu" => a.gpu.total_cmp(&b.gpu),
                _ => a.cpu.total_cmp(&b.cpu),
            };
            if sort.asc { ord } else { ord.reverse() }
        });
        groups
    }

    fn sync_selection_after_build(&mut self, groups: &[ProcessGroup], flat_procs: &[ProcessInfo]) {
        let Some(ref sel) = self.selected.clone() else {
            return;
        };
        match sel {
            SelectedTarget::Group { name, category, .. } => {
                if let Some(g) = groups
                    .iter()
                    .find(|g| &g.name == name && g.category == *category)
                {
                    self.selected = Some(SelectedTarget::Group {
                        name: g.name.clone(),
                        category: g.category,
                        pid: g.top_pid,
                        procs: g.procs.clone(),
                    });
                } else {
                    self.selected = None;
                }
            }
            SelectedTarget::Process { pid, .. } => {
                if !flat_procs.iter().any(|p| p.pid == *pid) {
                    self.selected = None;
                }
            }
            SelectedTarget::Startup { .. } => {}
        }
    }

    fn render_processes_tab(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let pal = *self.theme.palette();
        let total_mem = self
            .snap
            .system
            .as_ref()
            .map(|s| s.memory.total)
            .unwrap_or(0);
        let filtered = self.filtered_procs();
        let groups = self.build_groups(&filtered);
        self.sync_selection_after_build(&groups, &filtered);

        let searching = !self.search_text.trim().is_empty();
        let mut rows: Vec<ProcRow> = Vec::new();
        for cat in [
            ProcessCategory::App,
            ProcessCategory::Background,
            ProcessCategory::System,
        ] {
            let matching_indices: Vec<usize> = groups
                .iter()
                .enumerate()
                .filter(|(_, g)| g.category == cat)
                .map(|(i, _)| i)
                .collect();
            if matching_indices.is_empty() {
                continue;
            }
            let total_procs: usize = matching_indices
                .iter()
                .map(|&i| groups[i].procs.len())
                .sum();
            rows.push(ProcRow::Header {
                label: cat.label(),
                status: format!(
                    "{} process{}",
                    total_procs,
                    if total_procs == 1 { "" } else { "es" }
                ),
            });
            for g_idx in matching_indices {
                let g = &groups[g_idx];
                let open =
                    searching || self.expanded_groups.contains(&(g.name.clone(), g.category));
                rows.push(ProcRow::Group {
                    group_idx: g_idx,
                    open,
                });
                if open {
                    for p_idx in 0..g.procs.len() {
                        rows.push(ProcRow::Child {
                            group_idx: g_idx,
                            proc_idx: p_idx,
                        });
                    }
                }
            }
        }

        let mut toggle_expand: Option<(String, ProcessCategory)> = None;
        let mut new_selected: Option<SelectedTarget> = None;

        TableBuilder::new(ui)
            .striped(false)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::remainder().at_least(220.0))
            .column(Column::initial(130.0).at_least(90.0))
            .column(Column::initial(80.0).at_least(60.0))
            .column(Column::initial(120.0).at_least(90.0))
            .column(Column::initial(110.0).at_least(80.0))
            .column(Column::initial(110.0).at_least(80.0))
            .column(Column::initial(70.0).at_least(55.0))
            .sense(egui::Sense::click())
            .header(26.0, |mut header| {
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Processes, "Name", "name", true, false);
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Processes, "Status", "status", true, false);
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Processes, "CPU", "cpu", false, true);
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Processes, "Memory", "mem", false, true);
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Processes, "Disk", "disk", false, true);
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Processes, "Network", "net", false, true);
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Processes, "GPU", "gpu", false, true);
                });
            })
            .body(|body| {
                body.rows(26.0, rows.len(), |mut row| {
                    let r_idx = row.index();
                    match &rows[r_idx] {
                        ProcRow::Header { label, status } => {
                            row.col(|ui| {
                                ui.label(RichText::new(*label).strong().color(pal.accent));
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(status).color(pal.dim));
                            });
                            for _ in 0..5 {
                                row.col(|_| {});
                            }
                        }
                        ProcRow::Group { group_idx, open } => {
                            let g = &groups[*group_idx];
                            let is_sel = matches!(
                                &self.selected,
                                Some(SelectedTarget::Group { name, category, .. })
                                    if name == &g.name && *category == g.category
                            );
                            row.set_selected(is_sel);

                            let icon_name = self.desktop.icon_for_app(&g.name, &g.exe, &g.cmdline);
                            let tex = self.icons.get_texture(ctx, &icon_name);

                            let mut arrow_clicked = false;
                            row.col(|ui| {
                                let arrow = if *open { "▾" } else { "▸" };
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new(arrow).color(pal.dim))
                                            .frame(false),
                                    )
                                    .clicked()
                                {
                                    arrow_clicked = true;
                                }
                                if let Some(t) = tex {
                                    ui.image((t.id(), Vec2::splat(16.0)));
                                }
                                ui.label(RichText::new(&g.name).strong().color(pal.text));
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(&g.status).color(pal.text));
                            });
                            row.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let col = load_color(&pal, g.cpu, true);
                                    ui.label(RichText::new(fmt_pct(Some(g.cpu))).color(col));
                                });
                            });
                            row.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let pct = if total_mem > 0 {
                                        g.mem as f64 / total_mem as f64 * 100.0
                                    } else {
                                        0.0
                                    };
                                    let col = load_color(&pal, pct, false);
                                    ui.label(RichText::new(mem_text(g.mem, total_mem)).color(col))
                                        .on_hover_text(format!(
                                            "Total Group Memory: {}\n{} processes",
                                            fmt_bytes(Some(g.mem as f64), 1),
                                            g.procs.len()
                                        ));
                                });
                            });
                            row.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(fmt_rate(Some(g.disk as f64)));
                                });
                            });
                            row.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(fmt_rate(Some(g.net as f64)));
                                });
                            });
                            row.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(fmt_pct(Some(g.gpu)));
                                });
                            });

                            let resp = row.response();
                            if arrow_clicked || resp.double_clicked() {
                                toggle_expand = Some((g.name.clone(), g.category));
                            }
                            if resp.clicked() && !arrow_clicked {
                                new_selected = Some(SelectedTarget::Group {
                                    name: g.name.clone(),
                                    category: g.category,
                                    pid: g.top_pid,
                                    procs: g.procs.clone(),
                                });
                            }
                        }
                        ProcRow::Child {
                            group_idx,
                            proc_idx,
                        } => {
                            let p = &groups[*group_idx].procs[*proc_idx];
                            let is_sel = matches!(
                                &self.selected,
                                Some(SelectedTarget::Process { pid, .. }) if *pid == p.pid
                            );
                            row.set_selected(is_sel);

                            let icon_name = self.desktop.icon_for_app(&p.app, &p.exe, &p.cmdline);
                            let tex = self.icons.get_texture(ctx, &icon_name);

                            row.col(|ui| {
                                ui.add_space(22.0);
                                if let Some(t) = tex {
                                    ui.image((t.id(), Vec2::splat(16.0)));
                                }
                                ui.label(&p.name);
                            });
                            row.col(|ui| {
                                ui.label(&p.status);
                            });
                            row.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let col = load_color(&pal, p.cpu, true);
                                    ui.label(RichText::new(fmt_pct(Some(p.cpu))).color(col));
                                });
                            });
                            row.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let pct = if total_mem > 0 {
                                        p.mem as f64 / total_mem as f64 * 100.0
                                    } else {
                                        0.0
                                    };
                                    let col = load_color(&pal, pct, false);
                                    ui.label(RichText::new(mem_text(p.mem, total_mem)).color(col))
                                        .on_hover_text(format!(
                                            "Memory (PSS): {}\nResident (RSS): {}\nVirtual (VSZ): {}",
                                            fmt_bytes(Some(p.mem as f64), 1),
                                            fmt_bytes(Some(p.rss as f64), 1),
                                            fmt_bytes(Some(p.vmem as f64), 1),
                                        ));
                                });
                            });
                            row.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(fmt_rate(Some((p.disk_r + p.disk_w) as f64)));
                                });
                            });
                            row.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(fmt_rate(Some((p.net_up + p.net_down) as f64)));
                                });
                            });
                            row.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(fmt_pct(Some(p.gpu)));
                                });
                            });

                            if row.response().clicked() {
                                new_selected = Some(SelectedTarget::Process {
                                    pid: p.pid,
                                    name: p.name.clone(),
                                });
                            }
                        }
                    }
                });
            });

        if let Some(key) = toggle_expand {
            if !self.expanded_groups.remove(&key) {
                self.expanded_groups.insert(key);
            }
        }
        if let Some(sel) = new_selected {
            self.selected = Some(sel);
        }
    }

    fn render_details_tab(&mut self, ui: &mut egui::Ui) {
        let pal = *self.theme.palette();
        let total_mem = self
            .snap
            .system
            .as_ref()
            .map(|s| s.memory.total)
            .unwrap_or(0);
        let mut rows = self.filtered_procs();
        let sort = self.sorts.get(&Tab::Details).copied().unwrap_or(SortState {
            key: "pid",
            asc: true,
        });

        rows.sort_by(|a, b| {
            let ord = match sort.key {
                "name" => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                "status" => a.status.to_lowercase().cmp(&b.status.to_lowercase()),
                "user" => a.user.to_lowercase().cmp(&b.user.to_lowercase()),
                "cpu" => a.cpu.total_cmp(&b.cpu),
                "mem" => a.mem.cmp(&b.mem),
                "disk" => (a.disk_r + a.disk_w).cmp(&(b.disk_r + b.disk_w)),
                "net" => (a.net_up + a.net_down).cmp(&(b.net_up + b.net_down)),
                "gpu" => a.gpu.total_cmp(&b.gpu),
                "threads" => a.threads.cmp(&b.threads),
                "handles" => a.handles.cmp(&b.handles),
                "nice" => a.nice.cmp(&b.nice),
                "cmdline" => a.cmdline.to_lowercase().cmp(&b.cmdline.to_lowercase()),
                _ => a.pid.cmp(&b.pid),
            };
            if sort.asc { ord } else { ord.reverse() }
        });

        if let Some(SelectedTarget::Process { pid, .. }) = &self.selected {
            if !rows.iter().any(|p| p.pid == *pid) {
                self.selected = None;
            }
        }

        let mut new_selected: Option<SelectedTarget> = None;

        TableBuilder::new(ui)
            .striped(false)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::initial(70.0).at_least(50.0))
            .column(Column::initial(170.0).at_least(100.0))
            .column(Column::initial(95.0).at_least(70.0))
            .column(Column::initial(95.0).at_least(70.0))
            .column(Column::initial(65.0).at_least(55.0))
            .column(Column::initial(105.0).at_least(80.0))
            .column(Column::initial(95.0).at_least(70.0))
            .column(Column::initial(95.0).at_least(70.0))
            .column(Column::initial(60.0).at_least(50.0))
            .column(Column::initial(68.0).at_least(50.0))
            .column(Column::initial(72.0).at_least(55.0))
            .column(Column::initial(115.0).at_least(80.0))
            .column(Column::remainder().at_least(200.0))
            .sense(egui::Sense::click())
            .header(26.0, |mut header| {
                header
                    .col(|ui| self.sort_header_button(ui, Tab::Details, "PID", "pid", false, true));
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Details, "Name", "name", true, false)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Details, "Status", "status", true, false)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Details, "User", "user", true, false)
                });
                header
                    .col(|ui| self.sort_header_button(ui, Tab::Details, "CPU", "cpu", false, true));
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Details, "Memory", "mem", false, true)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Details, "Disk", "disk", false, true)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Details, "Network", "net", false, true)
                });
                header
                    .col(|ui| self.sort_header_button(ui, Tab::Details, "GPU", "gpu", false, true));
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Details, "Threads", "threads", false, true)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Details, "Handles", "handles", false, true)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Details, "Priority", "nice", false, true)
                });
                header.col(|ui| {
                    self.sort_header_button(
                        ui,
                        Tab::Details,
                        "Command line",
                        "cmdline",
                        true,
                        false,
                    )
                });
            })
            .body(|body| {
                body.rows(25.0, rows.len(), |mut row| {
                    let p = &rows[row.index()];
                    let is_sel = matches!(
                        &self.selected,
                        Some(SelectedTarget::Process { pid, .. }) if *pid == p.pid
                    );
                    row.set_selected(is_sel);

                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(p.pid.to_string());
                        });
                    });
                    row.col(|ui| {
                        ui.label(&p.name);
                    });
                    row.col(|ui| {
                        ui.label(&p.status);
                    });
                    row.col(|ui| {
                        ui.label(&p.user);
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let col = load_color(&pal, p.cpu, true);
                            ui.label(RichText::new(fmt_pct(Some(p.cpu))).color(col));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let pct = if total_mem > 0 {
                                p.mem as f64 / total_mem as f64 * 100.0
                            } else {
                                0.0
                            };
                            let col = load_color(&pal, pct, false);
                            ui.label(RichText::new(mem_text(p.mem, total_mem)).color(col))
                                .on_hover_text(format!(
                                    "Memory (PSS): {}\nResident (RSS): {}\nVirtual (VSZ): {}",
                                    fmt_bytes(Some(p.mem as f64), 1),
                                    fmt_bytes(Some(p.rss as f64), 1),
                                    fmt_bytes(Some(p.vmem as f64), 1),
                                ));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(fmt_rate(Some((p.disk_r + p.disk_w) as f64)));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(fmt_rate(Some((p.net_up + p.net_down) as f64)));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(fmt_pct(Some(p.gpu)));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(p.threads.to_string());
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(p.handles.to_string());
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(format!("{} · {}", p.nice, nice_label(p.nice)));
                        });
                    });
                    row.col(|ui| {
                        let cmd = if !p.cmdline.is_empty() {
                            &p.cmdline
                        } else if !p.exe.is_empty() {
                            &p.exe
                        } else {
                            "—"
                        };
                        ui.label(cmd);
                    });

                    if row.response().clicked() {
                        new_selected = Some(SelectedTarget::Process {
                            pid: p.pid,
                            name: p.name.clone(),
                        });
                    }
                });
            });

        if let Some(sel) = new_selected {
            self.selected = Some(sel);
        }
    }

    fn render_apps_tab(&mut self, ui: &mut egui::Ui) {
        let pal = *self.theme.palette();
        let note = format!(
            "Since {} · CPU time, network and disk totals are cumulative for this app session",
            fmt_clock(self.snap.started)
        );
        Frame::NONE
            .inner_margin(Margin::symmetric(14, 6))
            .stroke(Stroke::new(1.0, pal.border_soft))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(note).size(12.0).color(pal.dim));
            });

        let q = self.search_text.trim().to_lowercase();
        let mut rows: Vec<AppHistoryRow> = self
            .snap
            .apps
            .iter()
            .filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q))
            .cloned()
            .collect();
        let sort = self.sorts.get(&Tab::Apps).copied().unwrap_or(SortState {
            key: "cpu_s",
            asc: false,
        });
        rows.sort_by(|a, b| {
            let ord = match sort.key {
                "name" => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                "total_down" => a.total_down.cmp(&b.total_down),
                "total_up" => a.total_up.cmp(&b.total_up),
                "total_disk_r" => a.total_disk_r.cmp(&b.total_disk_r),
                "total_disk_w" => a.total_disk_w.cmp(&b.total_disk_w),
                "cpu" => a.cpu.total_cmp(&b.cpu),
                "mem" => a.mem.cmp(&b.mem),
                _ => a.cpu_s.total_cmp(&b.cpu_s),
            };
            if sort.asc { ord } else { ord.reverse() }
        });

        TableBuilder::new(ui)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::remainder().at_least(180.0))
            .column(Column::initial(100.0).at_least(80.0))
            .column(Column::initial(120.0).at_least(90.0))
            .column(Column::initial(110.0).at_least(85.0))
            .column(Column::initial(110.0).at_least(85.0))
            .column(Column::initial(110.0).at_least(85.0))
            .column(Column::initial(105.0).at_least(80.0))
            .column(Column::initial(125.0).at_least(95.0))
            .header(26.0, |mut header| {
                header
                    .col(|ui| self.sort_header_button(ui, Tab::Apps, "Name", "name", true, false));
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Apps, "CPU time", "cpu_s", false, true)
                });
                header.col(|ui| {
                    self.sort_header_button(
                        ui,
                        Tab::Apps,
                        "Network down",
                        "total_down",
                        false,
                        true,
                    )
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Apps, "Network up", "total_up", false, true)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Apps, "Disk read", "total_disk_r", false, true)
                });
                header.col(|ui| {
                    self.sort_header_button(
                        ui,
                        Tab::Apps,
                        "Disk write",
                        "total_disk_w",
                        false,
                        true,
                    )
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Apps, "Current CPU", "cpu", false, true)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Apps, "Current memory", "mem", false, true)
                });
            })
            .body(|body| {
                body.rows(25.0, rows.len(), |mut row| {
                    let r = &rows[row.index()];
                    row.col(|ui| {
                        ui.label(&r.name);
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(fmt_cpu_time(r.cpu_s));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(fmt_bytes(Some(r.total_down as f64), 1));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(fmt_bytes(Some(r.total_up as f64), 1));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(fmt_bytes(Some(r.total_disk_r as f64), 1));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(fmt_bytes(Some(r.total_disk_w as f64), 1));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(fmt_pct(Some(r.cpu)));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(fmt_bytes(Some(r.mem as f64), 1));
                        });
                    });
                });
            });
    }

    fn render_startup_tab(&mut self, ui: &mut egui::Ui) {
        let pal = *self.theme.palette();
        Frame::NONE
            .inner_margin(Margin::symmetric(14, 6))
            .stroke(Stroke::new(1.0, pal.border_soft))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new("User autostart entries and enabled systemd user services.")
                        .size(12.0)
                        .color(pal.dim),
                );
            });

        let data_opt = { self.startup_state.lock().unwrap().0.clone() };
        let Some(data) = data_opt else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Loading startup items…").color(pal.dim));
            });
            return;
        };

        let q = self.search_text.trim().to_lowercase();
        let mut rows: Vec<StartupRow> = data
            .into_iter()
            .filter(|r| {
                q.is_empty()
                    || r.name.to_lowercase().contains(&q)
                    || r.command.to_lowercase().contains(&q)
                    || r.description.to_lowercase().contains(&q)
            })
            .collect();

        let sort = self.sorts.get(&Tab::Startup).copied().unwrap_or(SortState {
            key: "type",
            asc: true,
        });
        rows.sort_by(|a, b| {
            let ord = match sort.key {
                "name" => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                "status" => a.status.to_lowercase().cmp(&b.status.to_lowercase()),
                "command" => {
                    let ac = if !a.command.is_empty() {
                        &a.command
                    } else {
                        &a.description
                    };
                    let bc = if !b.command.is_empty() {
                        &b.command
                    } else {
                        &b.description
                    };
                    ac.to_lowercase().cmp(&bc.to_lowercase())
                }
                _ => a
                    .entry_type
                    .to_lowercase()
                    .cmp(&b.entry_type.to_lowercase()),
            };
            if sort.asc { ord } else { ord.reverse() }
        });

        let mut toggled_row: Option<StartupRow> = None;
        let mut new_selected: Option<SelectedTarget> = None;

        TableBuilder::new(ui)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::initial(260.0).at_least(140.0))
            .column(Column::initial(85.0).at_least(70.0))
            .column(Column::initial(90.0).at_least(80.0))
            .column(Column::initial(130.0).at_least(95.0))
            .column(Column::remainder().at_least(200.0))
            .header(26.0, |mut header| {
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Startup, "Name", "name", true, false)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Startup, "Status", "status", true, false)
                });
                header.col(|ui| {
                    ui.label(RichText::new("Toggle").strong().color(pal.dim));
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Startup, "Type", "type", true, false)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Startup, "Command", "command", true, false)
                });
            })
            .body(|body| {
                body.rows(26.0, rows.len(), |mut row| {
                    let r = &rows[row.index()];
                    let is_enabled = r.status.eq_ignore_ascii_case("enabled");
                    let is_sel = matches!(
                        &self.selected,
                        Some(SelectedTarget::Startup { unit_or_file, name, .. })
                            if (!r.unit_or_file.is_empty() && unit_or_file == &r.unit_or_file)
                                || (unit_or_file.is_empty() && name == &r.name)
                    );
                    row.set_selected(is_sel);

                    row.col(|ui| {
                        ui.label(&r.name);
                    });
                    row.col(|ui| {
                        let (status_text, status_col) = if is_enabled {
                            ("Enabled", pal.green)
                        } else {
                            ("Disabled", pal.faint)
                        };
                        ui.label(RichText::new(status_text).color(status_col));
                    });
                    row.col(|ui| {
                        let (btn_text, btn_col) = if is_enabled {
                            ("Disable", pal.dim)
                        } else {
                            ("Enable", pal.accent)
                        };
                        let btn =
                            egui::Button::new(RichText::new(btn_text).size(12.0).color(btn_col))
                                .min_size(Vec2::new(65.0, 20.0));
                        if ui.add(btn).clicked() {
                            toggled_row = Some(r.clone());
                        }
                    });
                    row.col(|ui| {
                        ui.label(&r.entry_type);
                    });
                    row.col(|ui| {
                        let cmd = if !r.command.is_empty() {
                            &r.command
                        } else if !r.description.is_empty() {
                            &r.description
                        } else {
                            "—"
                        };
                        ui.label(cmd);
                    });

                    if row.response().clicked() && toggled_row.is_none() {
                        new_selected = Some(SelectedTarget::Startup {
                            name: r.name.clone(),
                            unit_or_file: r.unit_or_file.clone(),
                            status: r.status.clone(),
                            entry_type: r.entry_type.clone(),
                        });
                    }
                });
            });

        if let Some(sel) = new_selected {
            self.selected = Some(sel);
        }

        if let Some(r) = toggled_row {
            match self.sampler.toggle_startup(&r) {
                Ok(new_state) => {
                    // Optimistically update in memory for immediate UI response
                    let mut st = self.startup_state.lock().unwrap();
                    if let Some(ref mut list) = st.0 {
                        for item in list.iter_mut() {
                            if (!r.unit_or_file.is_empty() && item.unit_or_file == r.unit_or_file)
                                || (item.name == r.name && item.entry_type == r.entry_type)
                            {
                                item.status = if new_state {
                                    "Enabled".to_string()
                                } else {
                                    "Disabled".to_string()
                                };
                            }
                        }
                    }
                    drop(st);
                    self.ensure_startup(true);
                }
                Err(err) => {
                    self.error_dialog = Some(err);
                }
            }
        }
    }

    fn render_users_tab(&mut self, ui: &mut egui::Ui) {
        let q = self.search_text.trim().to_lowercase();
        let mut rows: Vec<UserRow> = self
            .snap
            .users
            .iter()
            .filter(|r| q.is_empty() || r.user.to_lowercase().contains(&q))
            .cloned()
            .collect();

        let sort = self.sorts.get(&Tab::Users).copied().unwrap_or(SortState {
            key: "cpu",
            asc: false,
        });
        rows.sort_by(|a, b| {
            let ord = match sort.key {
                "user" => a.user.to_lowercase().cmp(&b.user.to_lowercase()),
                "session" => a.session.to_lowercase().cmp(&b.session.to_lowercase()),
                "state" => a.state.to_lowercase().cmp(&b.state.to_lowercase()),
                "mem" => a.mem.cmp(&b.mem),
                "disk" => a.disk.cmp(&b.disk),
                "net" => (a.up + a.down).cmp(&(b.up + b.down)),
                "procs" => a.procs.cmp(&b.procs),
                _ => a.cpu.total_cmp(&b.cpu),
            };
            if sort.asc { ord } else { ord.reverse() }
        });

        TableBuilder::new(ui)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::remainder().at_least(150.0))
            .column(Column::initial(160.0).at_least(110.0))
            .column(Column::initial(100.0).at_least(75.0))
            .column(Column::initial(85.0).at_least(65.0))
            .column(Column::initial(120.0).at_least(90.0))
            .column(Column::initial(120.0).at_least(90.0))
            .column(Column::initial(120.0).at_least(90.0))
            .column(Column::initial(95.0).at_least(75.0))
            .header(26.0, |mut header| {
                header
                    .col(|ui| self.sort_header_button(ui, Tab::Users, "User", "user", true, false));
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Users, "Session", "session", true, false)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Users, "Status", "state", true, false)
                });
                header.col(|ui| self.sort_header_button(ui, Tab::Users, "CPU", "cpu", false, true));
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Users, "Memory", "mem", false, true)
                });
                header
                    .col(|ui| self.sort_header_button(ui, Tab::Users, "Disk", "disk", false, true));
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Users, "Network", "net", false, true)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Users, "Processes", "procs", false, true)
                });
            })
            .body(|body| {
                body.rows(25.0, rows.len(), |mut row| {
                    let r = &rows[row.index()];
                    row.col(|ui| {
                        ui.label(&r.user);
                    });
                    row.col(|ui| {
                        let ses = if !r.tty.is_empty() && r.tty != "—" {
                            format!("{} · {}", r.session, r.tty)
                        } else {
                            r.session.clone()
                        };
                        ui.label(ses);
                    });
                    row.col(|ui| {
                        ui.label(&r.state);
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(fmt_pct(Some(r.cpu)));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(fmt_bytes(Some(r.mem as f64), 1));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(fmt_rate(Some(r.disk as f64)));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(fmt_rate(Some((r.up + r.down) as f64)));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(r.procs.to_string());
                        });
                    });
                });
            });
    }

    fn render_services_tab(&mut self, ui: &mut egui::Ui) {
        let pal = *self.theme.palette();
        let data_opt = { self.services_state.lock().unwrap().0.clone() };
        let Some(data) = data_opt else {
            Frame::NONE
                .inner_margin(Margin::symmetric(14, 6))
                .stroke(Stroke::new(1.0, pal.border_soft))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Loading services…").size(12.0).color(pal.dim));
                });
            return;
        };

        let running = data
            .iter()
            .filter(|r| r.active == "active" && r.sub == "running")
            .count();
        let note = format!("{} running · {} total · systemd", running, data.len());
        Frame::NONE
            .inner_margin(Margin::symmetric(14, 6))
            .stroke(Stroke::new(1.0, pal.border_soft))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(note).size(12.0).color(pal.dim));
            });

        let q = self.search_text.trim().to_lowercase();
        let mut rows: Vec<ServiceRow> = data
            .into_iter()
            .filter(|r| {
                q.is_empty()
                    || r.unit.to_lowercase().contains(&q)
                    || r.description.to_lowercase().contains(&q)
            })
            .collect();

        let sort = self
            .sorts
            .get(&Tab::Services)
            .copied()
            .unwrap_or(SortState {
                key: "unit",
                asc: true,
            });
        rows.sort_by(|a, b| {
            let ord = match sort.key {
                "pid" => a
                    .pid
                    .map(|p| p as i64)
                    .unwrap_or(-1)
                    .cmp(&b.pid.map(|p| p as i64).unwrap_or(-1)),
                "description" => a
                    .description
                    .to_lowercase()
                    .cmp(&b.description.to_lowercase()),
                "status" => service_status(a).cmp(&service_status(b)),
                "startup" => service_startup(&a.startup).cmp(service_startup(&b.startup)),
                "scope" => a.scope.to_lowercase().cmp(&b.scope.to_lowercase()),
                _ => a.unit.to_lowercase().cmp(&b.unit.to_lowercase()),
            };
            if sort.asc { ord } else { ord.reverse() }
        });

        TableBuilder::new(ui)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::remainder().at_least(210.0))
            .column(Column::initial(75.0).at_least(55.0))
            .column(Column::initial(290.0).at_least(150.0))
            .column(Column::initial(130.0).at_least(90.0))
            .column(Column::initial(110.0).at_least(80.0))
            .column(Column::initial(85.0).at_least(65.0))
            .header(26.0, |mut header| {
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Services, "Name", "unit", true, false)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Services, "PID", "pid", false, true)
                });
                header.col(|ui| {
                    self.sort_header_button(
                        ui,
                        Tab::Services,
                        "Description",
                        "description",
                        true,
                        false,
                    )
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Services, "Status", "status", true, false)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Services, "Startup", "startup", true, false)
                });
                header.col(|ui| {
                    self.sort_header_button(ui, Tab::Services, "Scope", "scope", true, false)
                });
            })
            .body(|body| {
                body.rows(25.0, rows.len(), |mut row| {
                    let r = &rows[row.index()];
                    row.col(|ui| {
                        ui.label(&r.unit);
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let s = r
                                .pid
                                .map(|p| p.to_string())
                                .unwrap_or_else(|| "—".to_string());
                            ui.label(s);
                        });
                    });
                    row.col(|ui| {
                        ui.label(&r.description);
                    });
                    row.col(|ui| {
                        let col = if r.active == "failed" {
                            pal.danger
                        } else if r.active == "active" {
                            pal.green
                        } else {
                            pal.faint
                        };
                        ui.label(RichText::new(service_status(r)).color(col));
                    });
                    row.col(|ui| {
                        ui.label(service_startup(&r.startup));
                    });
                    row.col(|ui| {
                        ui.label(&r.scope);
                    });
                });
            });
    }

    fn render_performance_tab(&mut self, ui: &mut egui::Ui) {
        let pal = *self.theme.palette();
        let Some(ref sys) = self.snap.system else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Collecting performance data…").color(pal.dim));
            });
            return;
        };
        let hist = &self.snap.history;
        let disk0 = sys.disks.first();
        let wifi_iface = sys.net.interfaces.iter().find(|i| i.wireless);

        let disk_hist: Vec<f64> = hist
            .disk_r
            .iter()
            .zip(hist.disk_w.iter())
            .map(|(r, w)| r + w)
            .collect();
        let net_hist: Vec<f64> = hist
            .net_rx
            .iter()
            .zip(hist.net_tx.iter())
            .map(|(r, t)| r + t)
            .collect();

        // Left performance category selector panel
        egui::Panel::left("perf_nav")
            .resizable(false)
            .exact_size(224.0)
            .frame(
                Frame::NONE
                    .fill(pal.bg)
                    .inner_margin(Margin::symmetric(8, 8))
                    .stroke(Stroke::new(1.0, pal.border_soft)),
            )
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for item in PerfItem::ALL {
                    let selected = self.perf_item == item;
                    let val_text = match item {
                        PerfItem::Cpu => fmt_pct(Some(sys.cpu.util)),
                        PerfItem::Memory => fmt_pct(Some(sys.memory.percent)),
                        PerfItem::Disk => {
                            fmt_rate(Some(disk0.map(|d| d.read + d.write).unwrap_or(0.0)))
                        }
                        PerfItem::Wifi => {
                            fmt_rate(Some(wifi_iface.map(|i| i.rx + i.tx).unwrap_or(0.0)))
                        }
                        PerfItem::Gpu => sys
                            .gpu
                            .as_ref()
                            .map(|g| fmt_pct(Some(g.util)))
                            .unwrap_or_else(|| "—".to_string()),
                    };
                    let spark_data: &[f64] = match item {
                        PerfItem::Cpu => &hist.cpu,
                        PerfItem::Memory => &hist.mem,
                        PerfItem::Disk => &disk_hist,
                        PerfItem::Wifi => &net_hist,
                        PerfItem::Gpu => &hist.gpu,
                    };

                    let row_w = ui.available_width();
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(row_w, 56.0), egui::Sense::click());

                    let bg = if selected {
                        pal.accent_soft
                    } else if resp.hovered() {
                        pal.hover
                    } else {
                        Color32::TRANSPARENT
                    };
                    if bg != Color32::TRANSPARENT {
                        ui.painter().rect_filled(rect, CornerRadius::same(6), bg);
                    }
                    if selected {
                        let bar = egui::Rect::from_min_size(
                            egui::Pos2::new(rect.left() + 2.0, rect.top() + 10.0),
                            Vec2::new(3.0, rect.height() - 20.0),
                        );
                        ui.painter()
                            .rect_filled(bar, CornerRadius::same(2), item.color());
                    }

                    let inner = rect.shrink2(Vec2::new(10.0, 6.0));
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                    child.set_width(inner.width());
                    child.horizontal(|ui| {
                        ui.label(RichText::new(item.label()).strong().color(pal.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(val_text).color(pal.dim));
                        });
                    });
                    draw_sparkline(&mut child, spark_data, item.color(), 22.0);

                    if resp.clicked() {
                        self.perf_item = item;
                    }
                }
            });

        // Right detail scrollable panel
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(pal.bg))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Frame::NONE
                            .inner_margin(Margin::symmetric(20, 14))
                            .show(ui, |ui| {
                                let avail_w = ui.available_width().max(240.0);
                                ui.set_width(avail_w);

                                ui.label(
                                    RichText::new(self.perf_item.label())
                                        .size(23.0)
                                        .strong()
                                        .color(pal.text),
                                );

                                let subtitle = match self.perf_item {
                                    PerfItem::Cpu => format!(
                                        "{} · {} cores, {} logical processors",
                                        sys.cpu.model, sys.cpu.cores, sys.cpu.logical
                                    ),
                                    PerfItem::Memory => {
                                        let mut parts = Vec::new();
                                        parts.push(format!(
                                            "{} total",
                                            fmt_bytes(Some(sys.memory.total as f64), 1)
                                        ));
                                        parts.push(format!(
                                            "{} available",
                                            fmt_bytes(Some(sys.memory.available as f64), 1)
                                        ));
                                        if let Some(ref t) = sys.memory.mem_type {
                                            if let Some(spd) = sys.memory.speed_mts {
                                                parts.push(format!("{} {} MT/s", t, spd));
                                            } else {
                                                parts.push(t.clone());
                                            }
                                        } else if let Some(spd) = sys.memory.speed_mts {
                                            parts.push(format!("{} MT/s", spd));
                                        }
                                        if let Some(ref mfr) = sys.memory.manufacturer {
                                            parts.push(mfr.clone());
                                        }
                                        parts.join(" · ")
                                    }
                                    PerfItem::Disk => match disk0 {
                                        Some(d) => format!(
                                            "{} · {}",
                                            d.model,
                                            fmt_bytes(Some(d.size as f64), 1)
                                        ),
                                        None => "No disk".to_string(),
                                    },
                                    PerfItem::Wifi => match &sys.net.wifi {
                                        Some(w) => format!(
                                            "{} · {}",
                                            w.ssid.as_deref().unwrap_or("—"),
                                            w.iface
                                        ),
                                        None => "No wireless adapter".to_string(),
                                    },
                                    PerfItem::Gpu => sys
                                        .gpu
                                        .as_ref()
                                        .map(|g| g.name.clone())
                                        .unwrap_or_else(|| "No GPU".to_string()),
                                };
                                ui.add(
                                    egui::Label::new(RichText::new(subtitle).color(pal.dim)).wrap(),
                                );
                                ui.add_space(10.0);

                                // Chart card
                                Frame::NONE
                                    .fill(pal.panel)
                                    .stroke(Stroke::new(1.0, pal.border))
                                    .corner_radius(CornerRadius::same(8))
                                    .inner_margin(Margin::same(12))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        match self.perf_item {
                                            PerfItem::Cpu => {
                                                let s = [ChartSeries {
                                                    data: &hist.cpu,
                                                    color: PerfItem::Cpu.color(),
                                                    fill: true,
                                                }];
                                                draw_chart_area(
                                                    ui,
                                                    &pal,
                                                    &s,
                                                    Some(100.0),
                                                    |v| format!("{:.0}%", v),
                                                    215.0,
                                                );
                                                draw_legend(
                                                    ui,
                                                    &pal,
                                                    &[("Utilization", PerfItem::Cpu.color())],
                                                );
                                            }
                                            PerfItem::Memory => {
                                                let s = [ChartSeries {
                                                    data: &hist.mem,
                                                    color: PerfItem::Memory.color(),
                                                    fill: true,
                                                }];
                                                draw_chart_area(
                                                    ui,
                                                    &pal,
                                                    &s,
                                                    Some(100.0),
                                                    |v| format!("{:.0}%", v),
                                                    215.0,
                                                );
                                                draw_legend(
                                                    ui,
                                                    &pal,
                                                    &[("In use", PerfItem::Memory.color())],
                                                );
                                            }
                                            PerfItem::Disk => {
                                                let s = [
                                                    ChartSeries {
                                                        data: &hist.disk_r,
                                                        color: pal.green,
                                                        fill: true,
                                                    },
                                                    ChartSeries {
                                                        data: &hist.disk_w,
                                                        color: pal.amber,
                                                        fill: false,
                                                    },
                                                ];
                                                draw_chart_area(
                                                    ui,
                                                    &pal,
                                                    &s,
                                                    None,
                                                    |v| format!("{}/s", fmt_bytes(Some(v), 0)),
                                                    215.0,
                                                );
                                                draw_legend(
                                                    ui,
                                                    &pal,
                                                    &[("Read", pal.green), ("Write", pal.amber)],
                                                );
                                            }
                                            PerfItem::Wifi => {
                                                let s = [
                                                    ChartSeries {
                                                        data: &hist.net_rx,
                                                        color: pal.accent,
                                                        fill: true,
                                                    },
                                                    ChartSeries {
                                                        data: &hist.net_tx,
                                                        color: pal.amber,
                                                        fill: false,
                                                    },
                                                ];
                                                draw_chart_area(
                                                    ui,
                                                    &pal,
                                                    &s,
                                                    None,
                                                    |v| format!("{}/s", fmt_bytes(Some(v), 0)),
                                                    215.0,
                                                );
                                                draw_legend(
                                                    ui,
                                                    &pal,
                                                    &[("Receive", pal.accent), ("Send", pal.amber)],
                                                );
                                            }
                                            PerfItem::Gpu => {
                                                let s = [ChartSeries {
                                                    data: &hist.gpu,
                                                    color: PerfItem::Gpu.color(),
                                                    fill: true,
                                                }];
                                                draw_chart_area(
                                                    ui,
                                                    &pal,
                                                    &s,
                                                    Some(100.0),
                                                    |v| format!("{:.0}%", v),
                                                    215.0,
                                                );
                                                draw_legend(
                                                    ui,
                                                    &pal,
                                                    &[("Utilization", PerfItem::Gpu.color())],
                                                );
                                            }
                                        }
                                    });

                                ui.add_space(16.0);

                                // Stats grid: explicit equal-width 4-column rows
                                let stats =
                                    build_perf_stats(sys, &self.snap.totals, self.perf_item);
                                let gap = 16.0_f32;
                                let col_w = ((avail_w - 3.0 * gap) / 4.0).max(100.0);
                                for chunk in stats.chunks(4) {
                                    ui.horizontal_top(|ui| {
                                        ui.spacing_mut().item_spacing.x = gap;
                                        for (lbl, val, sub) in chunk {
                                            ui.allocate_ui_with_layout(
                                                Vec2::new(col_w, 0.0),
                                                Layout::top_down(Align::LEFT),
                                                |ui| {
                                                    ui.set_width(col_w);
                                                    ui.label(
                                                        RichText::new(lbl)
                                                            .size(12.0)
                                                            .color(pal.dim),
                                                    );
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(val)
                                                                .size(18.0)
                                                                .strong()
                                                                .color(pal.text),
                                                        )
                                                        .wrap(),
                                                    );
                                                    if !sub.is_empty() {
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new(sub)
                                                                    .size(11.0)
                                                                    .color(pal.faint),
                                                            )
                                                            .wrap(),
                                                        );
                                                    }
                                                },
                                            );
                                        }
                                    });
                                    ui.add_space(10.0);
                                }

                                ui.add_space(6.0);

                                // Extra details per perf item
                                match self.perf_item {
                                    PerfItem::Cpu => {
                                        ui.label(
                                            RichText::new("Logical processors")
                                                .size(13.0)
                                                .strong()
                                                .color(pal.text),
                                        );
                                        ui.add_space(6.0);
                                        let core_gap = 8.0_f32;
                                        let cols =
                                            ((avail_w + core_gap) / 138.0).floor().clamp(2.0, 6.0)
                                                as usize;
                                        let cell_w = ((avail_w - (cols as f32 - 1.0) * core_gap)
                                            / cols as f32)
                                            .max(100.0);
                                        for (row_idx, chunk) in
                                            sys.cpu.per_core.chunks(cols).enumerate()
                                        {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = core_gap;
                                                for (offset, &pct) in chunk.iter().enumerate() {
                                                    let i = row_idx * cols + offset;
                                                    let core_hist: Vec<f64> = hist
                                                        .cores
                                                        .iter()
                                                        .filter_map(|r| r.get(i).copied())
                                                        .collect();
                                                    let val_str = if let Some(&f_mhz) =
                                                        sys.cpu.freqs.get(i)
                                                    {
                                                        format!(
                                                            "{:.2}G · {}",
                                                            f_mhz / 1000.0,
                                                            fmt_pct(Some(pct))
                                                        )
                                                    } else {
                                                        fmt_pct(Some(pct))
                                                    };
                                                    Frame::NONE
                                                        .fill(pal.panel)
                                                        .stroke(Stroke::new(1.0, pal.border))
                                                        .corner_radius(CornerRadius::same(6))
                                                        .inner_margin(Margin::same(6))
                                                        .show(ui, |ui| {
                                                            let inner_w = (cell_w - 14.0).max(70.0);
                                                            ui.set_width(inner_w);
                                                            ui.horizontal(|ui| {
                                                                ui.label(
                                                                    RichText::new(format!(
                                                                        "CPU {}",
                                                                        i
                                                                    ))
                                                                    .size(11.0)
                                                                    .color(pal.dim),
                                                                );
                                                                ui.with_layout(
                                                                    Layout::right_to_left(
                                                                        Align::Center,
                                                                    ),
                                                                    |ui| {
                                                                        ui.label(
                                                                            RichText::new(val_str)
                                                                                .size(11.0)
                                                                                .color(pal.dim),
                                                                        );
                                                                    },
                                                                );
                                                            });
                                                            draw_sparkline(
                                                                ui, &core_hist, pal.accent, 22.0,
                                                            );
                                                        });
                                                }
                                            });
                                            ui.add_space(core_gap);
                                        }
                                    }
                                    PerfItem::Memory => {
                                        let m = &sys.memory;
                                        Frame::NONE
                                            .fill(pal.panel)
                                            .stroke(Stroke::new(1.0, pal.border))
                                            .corner_radius(CornerRadius::same(8))
                                            .inner_margin(Margin::same(12))
                                            .show(ui, |ui| {
                                                ui.set_width(ui.available_width());
                                                ui.label(
                                                    RichText::new("Composition")
                                                        .size(13.0)
                                                        .strong()
                                                        .color(pal.text),
                                                );
                                                ui.add_space(6.0);
                                                let free_b = m
                                                    .total
                                                    .saturating_sub(m.used)
                                                    .saturating_sub(m.cached);
                                                draw_composition_bar(
                                                    ui,
                                                    &pal,
                                                    m.used as f64,
                                                    m.cached as f64,
                                                    free_b as f64,
                                                );
                                                ui.add_space(4.0);
                                                draw_legend(
                                                    ui,
                                                    &pal,
                                                    &[
                                                        ("In use", pal.accent),
                                                        ("Cached", pal.purple),
                                                        ("Free", pal.border),
                                                    ],
                                                );
                                            });
                                    }
                                    PerfItem::Disk if !sys.volumes.is_empty() => {
                                        Frame::NONE
                                            .fill(pal.panel)
                                            .stroke(Stroke::new(1.0, pal.border))
                                            .corner_radius(CornerRadius::same(8))
                                            .inner_margin(Margin::same(12))
                                            .show(ui, |ui| {
                                                ui.set_width(ui.available_width());
                                                ui.label(
                                                    RichText::new("Volumes")
                                                        .size(13.0)
                                                        .strong()
                                                        .color(pal.text),
                                                );
                                                ui.add_space(6.0);
                                                egui::Grid::new("volumes_grid")
                                                    .num_columns(7)
                                                    .spacing([16.0, 6.0])
                                                    .show(ui, |ui| {
                                                        for (idx, h) in [
                                                            "Volume", "Mount", "Type", "Used",
                                                            "Free", "Total", "",
                                                        ]
                                                        .iter()
                                                        .enumerate()
                                                        {
                                                            let lbl = RichText::new(*h)
                                                                .size(12.0)
                                                                .color(pal.faint);
                                                            if (3..=5).contains(&idx) {
                                                                ui.with_layout(
                                                                    Layout::right_to_left(
                                                                        Align::Center,
                                                                    ),
                                                                    |ui| {
                                                                        ui.label(lbl);
                                                                    },
                                                                );
                                                            } else {
                                                                ui.label(lbl);
                                                            }
                                                        }
                                                        ui.end_row();
                                                        for v in &sys.volumes {
                                                            ui.label(
                                                                RichText::new(&v.device).size(12.0),
                                                            );
                                                            ui.label(
                                                                RichText::new(&v.mount).size(12.0),
                                                            );
                                                            ui.label(
                                                                RichText::new(&v.fstype).size(12.0),
                                                            );
                                                            ui.with_layout(
                                                                Layout::right_to_left(
                                                                    Align::Center,
                                                                ),
                                                                |ui| {
                                                                    ui.label(
                                                                        RichText::new(fmt_bytes(
                                                                            Some(v.used as f64),
                                                                            1,
                                                                        ))
                                                                        .size(12.0),
                                                                    );
                                                                },
                                                            );
                                                            ui.with_layout(
                                                                Layout::right_to_left(
                                                                    Align::Center,
                                                                ),
                                                                |ui| {
                                                                    ui.label(
                                                                        RichText::new(fmt_bytes(
                                                                            Some(v.free as f64),
                                                                            1,
                                                                        ))
                                                                        .size(12.0),
                                                                    );
                                                                },
                                                            );
                                                            ui.with_layout(
                                                                Layout::right_to_left(
                                                                    Align::Center,
                                                                ),
                                                                |ui| {
                                                                    ui.label(
                                                                        RichText::new(fmt_bytes(
                                                                            Some(v.total as f64),
                                                                            1,
                                                                        ))
                                                                        .size(12.0),
                                                                    );
                                                                },
                                                            );
                                                            let frac = (v.percent / 100.0)
                                                                .clamp(0.0, 1.0)
                                                                as f32;
                                                            draw_thin_bar(
                                                                ui, &pal, frac, pal.accent, 110.0,
                                                            );
                                                            ui.end_row();
                                                        }
                                                    });
                                            });
                                    }
                                    PerfItem::Gpu => {
                                        let mut gpu_procs: Vec<&ProcessInfo> = self
                                            .snap
                                            .processes
                                            .iter()
                                            .filter(|p| p.gpu > 0.0)
                                            .collect();
                                        gpu_procs.sort_by(|a, b| b.gpu.total_cmp(&a.gpu));
                                        gpu_procs.truncate(6);
                                        if !gpu_procs.is_empty() {
                                            Frame::NONE
                                                .fill(pal.panel)
                                                .stroke(Stroke::new(1.0, pal.border))
                                                .corner_radius(CornerRadius::same(8))
                                                .inner_margin(Margin::same(12))
                                                .show(ui, |ui| {
                                                    ui.set_width(ui.available_width());
                                                    ui.label(
                                                        RichText::new("Processes using the GPU")
                                                            .size(13.0)
                                                            .strong()
                                                            .color(pal.text),
                                                    );
                                                    ui.add_space(6.0);
                                                    egui::Grid::new("gpu_procs_grid")
                                                        .num_columns(3)
                                                        .spacing([14.0, 7.0])
                                                        .show(ui, |ui| {
                                                            for p in gpu_procs {
                                                                let eng = p
                                                                    .gpu_engine
                                                                    .as_ref()
                                                                    .map(|e| format!(" · {}", e))
                                                                    .unwrap_or_default();
                                                                ui.label(
                                                                    RichText::new(format!(
                                                                        "{}  ({}{})",
                                                                        p.name, p.pid, eng
                                                                    ))
                                                                    .size(12.0),
                                                                );
                                                                draw_thin_bar(
                                                                    ui,
                                                                    &pal,
                                                                    (p.gpu / 100.0).clamp(0.0, 1.0)
                                                                        as f32,
                                                                    PerfItem::Gpu.color(),
                                                                    160.0,
                                                                );
                                                                ui.label(
                                                                    RichText::new(fmt_pct(Some(
                                                                        p.gpu,
                                                                    )))
                                                                    .size(12.0)
                                                                    .color(pal.dim),
                                                                );
                                                                ui.end_row();
                                                            }
                                                        });
                                                });
                                        }
                                    }
                                    _ => {}
                                }
                            });
                    });
            });
    }

    fn render_modals(&mut self, ctx: &egui::Context) {
        if let Some(dlg) = self.confirm_dialog.as_ref() {
            let title = dlg.title.clone();
            let detail = dlg.detail.clone();
            let pids = dlg.pids.clone();
            let mode = dlg.mode;
            let action = match mode {
                KillMode::End => "End task",
                KillMode::Kill => "Kill task",
            };
            let mut close = false;
            let mut do_kill = false;

            egui::Window::new(action)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_min_width(300.0);
                    ui.label(RichText::new(&title).strong().size(14.0));
                    ui.add_space(4.0);
                    ui.label(RichText::new(&detail).color(self.theme.palette().dim));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let action_lbl =
                                RichText::new(action).color(self.theme.palette().danger);
                            if ui.button(action_lbl).clicked() {
                                do_kill = true;
                                close = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    });
                });

            if do_kill {
                let result = match mode {
                    KillMode::End => terminate_pids(&pids),
                    KillMode::Kill => kill_pids(&pids),
                };
                if let Err(err) = result {
                    self.error_dialog = Some(err);
                }
                self.selected = None;
                self.snap = self.sampler.snapshot();
            }
            if close {
                self.confirm_dialog = None;
            }
        }

        if let Some(msg) = self.error_dialog.clone() {
            let mut close = false;
            egui::Window::new("ProcHarbor")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_min_width(280.0);
                    ui.label(&msg);
                    ui.add_space(10.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("OK").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.error_dialog = None;
            }
        }
    }
}

impl eframe::App for ProcHarborApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_shortcuts(&ctx);
        self.refresh_snapshot_if_needed();

        self.render_header(ui);
        self.render_sidebar(ui);

        let pal = self.theme.palette();
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(pal.bg))
            .show(ui, |ui| {
                self.render_summary(ui);
                match self.current_tab {
                    Tab::Processes => self.render_processes_tab(&ctx, ui),
                    Tab::Performance => self.render_performance_tab(ui),
                    Tab::Apps => self.render_apps_tab(ui),
                    Tab::Startup => self.render_startup_tab(ui),
                    Tab::Users => self.render_users_tab(ui),
                    Tab::Details => self.render_details_tab(ui),
                    Tab::Services => self.render_services_tab(ui),
                }
            });

        self.render_modals(&ctx);
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.sampler.stop_all();
    }
}

fn summary_pair(ui: &mut egui::Ui, pal: &Palette, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        ui.label(RichText::new(label).size(12.0).color(pal.dim));
        ui.label(RichText::new(value).size(12.0).strong().color(pal.text));
    });
}

fn mem_text(mem: u64, total_mem: u64) -> String {
    if total_mem > 0 {
        format!(
            "{} · {:.0}%",
            fmt_bytes(Some(mem as f64), 1),
            mem as f64 / total_mem as f64 * 100.0
        )
    } else {
        fmt_bytes(Some(mem as f64), 1)
    }
}

fn service_status(r: &ServiceRow) -> String {
    match r.active.as_str() {
        "failed" => "Failed".to_string(),
        "active" => {
            if r.sub == "running" {
                "Running".to_string()
            } else {
                format!("Active ({})", r.sub)
            }
        }
        "activating" => "Starting".to_string(),
        "deactivating" => "Stopping".to_string(),
        _ => "Stopped".to_string(),
    }
}

fn service_startup(state: &str) -> &'static str {
    match state {
        "enabled" => "Automatic",
        "disabled" => "Disabled",
        "static" | "indirect" | "generated" => "Manual",
        _ => "—",
    }
}

fn draw_legend(ui: &mut egui::Ui, pal: &Palette, items: &[(&str, Color32)]) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        for (label, color) in items {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.label(RichText::new("■").size(12.0).color(*color));
                ui.label(RichText::new(*label).size(12.0).color(pal.dim));
            });
        }
    });
}

fn build_perf_stats(
    sys: &crate::sampler::SystemInfo,
    totals: &crate::sampler::Totals,
    item: PerfItem,
) -> Vec<(String, String, String)> {
    match item {
        PerfItem::Cpu => {
            let cpu = &sys.cpu;
            let max_cur = cpu.freqs.iter().copied().fold(0.0_f64, f64::max) / 1000.0;
            let turbo_str = if cpu.no_turbo == Some(1) { "off" } else { "on" };
            vec![
                (
                    "Utilization".into(),
                    fmt_pct(Some(cpu.util)),
                    format!("load {:.2} · {:.2}", cpu.load[0], cpu.load[1]),
                ),
                (
                    "Speed".into(),
                    format!("{:.2} GHz", cpu.freq / 1000.0),
                    format!("max core {:.2} GHz · turbo {}", max_cur, turbo_str),
                ),
                (
                    "Processes".into(),
                    totals.processes.to_string(),
                    String::new(),
                ),
                ("Threads".into(), totals.threads.to_string(), String::new()),
                ("Handles".into(), totals.handles.to_string(), String::new()),
                ("Uptime".into(), fmt_uptime(sys.uptime), String::new()),
                (
                    "Base speed".into(),
                    format!("{:.2} GHz", cpu.base_mhz / 1000.0),
                    format!(
                        "P-core hardware · E-core {:.2} GHz",
                        cpu.base_e_mhz / 1000.0
                    ),
                ),
                (
                    "Max speed".into(),
                    format!("{:.2} GHz", cpu.max_policy_mhz / 1000.0),
                    if cpu.no_turbo == Some(1) {
                        "policy cap · turbo off".into()
                    } else {
                        "policy cap".into()
                    },
                ),
                ("Cores".into(), cpu.cores.to_string(), String::new()),
                (
                    "Logical processors".into(),
                    cpu.logical.to_string(),
                    String::new(),
                ),
                ("Sockets".into(), cpu.sockets.to_string(), String::new()),
                (
                    "Virtualization".into(),
                    if cpu.virtualization {
                        "Enabled"
                    } else {
                        "Disabled"
                    }
                    .into(),
                    String::new(),
                ),
                (
                    "Temperature".into(),
                    cpu.temp
                        .map(|t| format!("{:.0} °C", t))
                        .unwrap_or_else(|| "—".into()),
                    "package".into(),
                ),
            ]
        }
        PerfItem::Memory => {
            let m = &sys.memory;
            let used_pct = if m.total > 0 {
                m.used as f64 / m.total as f64 * 100.0
            } else {
                0.0
            };
            let speed_val = m
                .speed_mts
                .map(|s| format!("{} MT/s", s))
                .unwrap_or_else(|| "—".into());
            let slots_val = if m.slots_total > 0 {
                format!("{} of {}", m.slots_used, m.slots_total)
            } else {
                "—".into()
            };
            let form_val = m.form_factor.clone().unwrap_or_else(|| "—".into());
            let hw_res_val = if m.hw_reserved > 0 {
                fmt_bytes(Some(m.hw_reserved as f64), 1)
            } else {
                "—".into()
            };
            let type_val = m.mem_type.clone().unwrap_or_else(|| "—".into());
            let mfr_val = m.manufacturer.clone().unwrap_or_else(|| "—".into());
            let installed_val = if m.installed > 0 {
                fmt_bytes(Some(m.installed as f64), 1)
            } else {
                fmt_bytes(Some(m.total as f64), 1)
            };

            vec![
                (
                    "In use".into(),
                    fmt_bytes(Some(m.used as f64), 1),
                    format!("{:.1}%", used_pct),
                ),
                (
                    "Available".into(),
                    fmt_bytes(Some(m.available as f64), 1),
                    String::new(),
                ),
                (
                    "Committed".into(),
                    fmt_bytes(Some(m.committed as f64), 1),
                    format!("of {}", fmt_bytes(Some(m.commit_limit as f64), 1)),
                ),
                (
                    "Cached".into(),
                    fmt_bytes(Some(m.cached as f64), 1),
                    String::new(),
                ),
                ("Speed".into(), speed_val, String::new()),
                ("Slots used".into(), slots_val, String::new()),
                ("Form factor".into(), form_val, String::new()),
                ("Hardware reserved".into(), hw_res_val, String::new()),
                ("Memory type".into(), type_val, String::new()),
                ("Manufacturer".into(), mfr_val, String::new()),
                (
                    "Installed RAM".into(),
                    installed_val,
                    m.max_capacity
                        .map(|c| format!("max {}", fmt_bytes(Some(c as f64), 0)))
                        .unwrap_or_default(),
                ),
                (
                    "Swap used".into(),
                    fmt_bytes(Some(m.swap_used as f64), 1),
                    format!("of {}", fmt_bytes(Some(m.swap_total as f64), 1)),
                ),
                (
                    "Paged pool".into(),
                    fmt_bytes(Some(m.slab as f64), 1),
                    "Slab cache".into(),
                ),
                (
                    "Non-paged pool".into(),
                    fmt_bytes(Some(m.kernel as f64), 1),
                    "kernel stack + page tables".into(),
                ),
                (
                    "Dirty".into(),
                    fmt_bytes(Some(m.dirty as f64), 1),
                    String::new(),
                ),
                (
                    "Writeback".into(),
                    fmt_bytes(Some(m.writeback as f64), 1),
                    String::new(),
                ),
            ]
        }
        PerfItem::Disk => {
            let d = sys.disks.first();
            let root_vol = sys
                .volumes
                .iter()
                .find(|v| v.mount == "/")
                .or_else(|| sys.volumes.first());
            vec![
                (
                    "Active time".into(),
                    fmt_pct(d.map(|x| x.active)),
                    String::new(),
                ),
                (
                    "Average response".into(),
                    format!("{:.1} ms", d.map(|x| x.response).unwrap_or(0.0)),
                    String::new(),
                ),
                (
                    "Read speed".into(),
                    fmt_rate(d.map(|x| x.read)),
                    String::new(),
                ),
                (
                    "Write speed".into(),
                    fmt_rate(d.map(|x| x.write)),
                    String::new(),
                ),
                (
                    "IOPS".into(),
                    fmt_thousands(d.map(|x| x.iops.round() as u64).unwrap_or(0)),
                    String::new(),
                ),
                (
                    "Capacity used".into(),
                    root_vol
                        .map(|v| {
                            format!(
                                "{} / {}",
                                fmt_bytes(Some(v.used as f64), 1),
                                fmt_bytes(Some(v.total as f64), 1)
                            )
                        })
                        .unwrap_or_else(|| "—".into()),
                    root_vol
                        .map(|v| format!("{:.0}%", v.percent))
                        .unwrap_or_default(),
                ),
                (
                    "Model".into(),
                    d.map(|x| x.model.clone()).unwrap_or_else(|| "—".into()),
                    String::new(),
                ),
                (
                    "Temperature".into(),
                    d.and_then(|x| x.temp)
                        .map(|t| format!("{:.0} °C", t))
                        .unwrap_or_else(|| "—".into()),
                    "NVMe".into(),
                ),
            ]
        }
        PerfItem::Wifi => match &sys.net.wifi {
            Some(w) => vec![
                (
                    "SSID".into(),
                    w.ssid.clone().unwrap_or_else(|| "—".into()),
                    String::new(),
                ),
                (
                    "Signal".into(),
                    w.signal_dbm
                        .map(|d| format!("{:.0} dBm", d))
                        .unwrap_or_else(|| "—".into()),
                    w.signal_pct
                        .map(|p| format!("{:.0}%", p))
                        .unwrap_or_default(),
                ),
                (
                    "Link speed".into(),
                    w.link_mbps
                        .map(|m| format!("{:.0} Mbps", m))
                        .unwrap_or_else(|| "—".into()),
                    String::new(),
                ),
                (
                    "Channel".into(),
                    w.channel
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "—".into()),
                    w.freq_ghz.map(|f| format!("{} GHz", f)).unwrap_or_default(),
                ),
                (
                    "Adapter".into(),
                    w.iface.clone(),
                    w.driver.clone().unwrap_or_default(),
                ),
                (
                    "IPv4".into(),
                    w.ipv4.clone().unwrap_or_else(|| "—".into()),
                    String::new(),
                ),
                (
                    "MAC".into(),
                    if w.mac.is_empty() {
                        "—".into()
                    } else {
                        w.mac.clone()
                    },
                    String::new(),
                ),
                (
                    "Transmit power".into(),
                    w.txpower
                        .map(|p| format!("{} dBm", p))
                        .unwrap_or_else(|| "—".into()),
                    String::new(),
                ),
                (
                    "Power save".into(),
                    w.powersave.clone().unwrap_or_else(|| "—".into()),
                    String::new(),
                ),
                (
                    "Radio temp".into(),
                    w.temp
                        .map(|t| format!("{:.0} °C", t))
                        .unwrap_or_else(|| "—".into()),
                    String::new(),
                ),
            ],
            None => vec![("Adapter".into(), "Not found".into(), String::new())],
        },
        PerfItem::Gpu => match &sys.gpu {
            Some(g) => vec![
                ("Utilization".into(), fmt_pct(Some(g.util)), String::new()),
                (
                    "GPU frequency".into(),
                    g.freq
                        .map(|f| format!("{} MHz", f))
                        .unwrap_or_else(|| "—".into()),
                    g.freq_max
                        .map(|f| format!("max {} MHz", f))
                        .unwrap_or_default(),
                ),
                (
                    "Active frequency".into(),
                    g.freq_act
                        .map(|f| format!("{} MHz", f))
                        .unwrap_or_else(|| "—".into()),
                    "actual clock".into(),
                ),
                ("Driver".into(), g.driver.clone(), String::new()),
                ("Name".into(), g.name.clone(), String::new()),
                (
                    "Memory".into(),
                    "Shared".into(),
                    "integrated GPU uses system memory".into(),
                ),
            ],
            None => vec![("GPU".into(), "Not found".into(), String::new())],
        },
    }
}
