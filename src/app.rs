use crate::system_monitor::{
    format_bytes, format_duration, format_rate_bps, AppGroupEntry, ProcessEntry, SystemMonitor,
    HISTORY_LEN,
};
use crate::theme::{apply_winui_style, install_windows_fonts, ThemeMode, WinUiPalette};
use egui::{
    Align, Color32, CursorIcon, Frame, Id, Key, Layout, Margin, Pos2, Rect, RichText, Rounding,
    Sense, Stroke, Vec2, ViewportCommand,
};
use egui_extras::{Column, TableBuilder};
use egui_plot::{Line, Plot, PlotPoints};
use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavPage {
    Processes,
    Performance,
    ThisPc,
    StartupApps,
    Users,
    Details,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PerfResource {
    Cpu,
    Memory,
    Disk,
    Network,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortColumn {
    Name,
    Pid,
    Category,
    Cpu,
    Memory,
    Disk,
    RunTime,
}

#[derive(Clone, Debug)]
enum DisplayRow {
    Group(AppGroupEntry),
    Child(ProcessEntry),
    Flat(ProcessEntry),
}

pub struct TaskManagerApp {
    pub monitor: SystemMonitor,
    pub theme_mode: ThemeMode,
    pub mica_active: bool,
    pub nav_page: NavPage,
    pub sidebar_expanded: bool,
    pub perf_resource: PerfResource,
    pub cpu_per_core_view: bool,

    // Grouping, Search & Sorting
    pub group_by_app: bool,
    pub apps_first: bool,
    pub expanded_groups: HashSet<String>,
    pub search_query: String,
    pub sort_col: SortColumn,
    pub sort_desc: bool,
    pub selected_pid: Option<u32>,
    pub selected_group: Option<String>,

    // Settings & Dialogs
    pub show_heatmap: bool,
    pub always_on_top: bool,
    pub show_run_task_modal: bool,
    pub run_task_input: String,
    pub run_task_as_admin: bool,
    pub status_toast: Option<(String, bool, Instant)>,
}

impl TaskManagerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, mica_active: bool) -> Self {
        install_windows_fonts(&cc.egui_ctx);
        let theme_mode = ThemeMode::Dark;
        apply_winui_style(&cc.egui_ctx, theme_mode, mica_active);

        Self {
            monitor: SystemMonitor::new(),
            theme_mode,
            mica_active,
            nav_page: NavPage::Processes,
            sidebar_expanded: true,
            perf_resource: PerfResource::Cpu,
            cpu_per_core_view: false,
            group_by_app: true,
            apps_first: true,
            expanded_groups: HashSet::new(),
            search_query: String::new(),
            sort_col: SortColumn::Memory,
            sort_desc: true,
            selected_pid: None,
            selected_group: None,
            show_heatmap: true,
            always_on_top: false,
            show_run_task_modal: false,
            run_task_input: String::new(),
            run_task_as_admin: false,
            status_toast: None,
        }
    }

    fn set_toast(&mut self, msg: String, is_error: bool) {
        self.status_toast = Some((msg, is_error, Instant::now()));
    }

    fn build_process_rows(&self) -> Vec<DisplayRow> {
        let query = self.search_query.trim().to_lowercase();

        if self.group_by_app {
            let mut groups: Vec<AppGroupEntry> = self
                .monitor
                .app_groups
                .iter()
                .filter(|g| {
                    if query.is_empty() {
                        return true;
                    }
                    g.display_name.to_lowercase().contains(&query)
                        || g.group_key.contains(&query)
                        || g.primary_pid.to_string().contains(&query)
                        || g.exe_path.to_lowercase().contains(&query)
                        || g.children.iter().any(|c| {
                            c.name.to_lowercase().contains(&query)
                                || c.pid.to_string().contains(&query)
                        })
                })
                .cloned()
                .collect();

            groups.sort_by(|a, b| {
                // Keep User Apps (Chrome, WhatsApp, Spotify, etc.) at the top when apps_first is active
                if self.apps_first && a.is_user_app != b.is_user_app {
                    return b.is_user_app.cmp(&a.is_user_app);
                }

                let ord = match self.sort_col {
                    SortColumn::Name => a
                        .display_name
                        .to_lowercase()
                        .cmp(&b.display_name.to_lowercase()),
                    SortColumn::Pid => a.primary_pid.cmp(&b.primary_pid),
                    SortColumn::Category => a.is_user_app.cmp(&b.is_user_app),
                    SortColumn::Cpu => a
                        .cpu_pct
                        .partial_cmp(&b.cpu_pct)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    SortColumn::Memory => a.memory_bytes.cmp(&b.memory_bytes),
                    SortColumn::Disk => a.disk_total_bps.cmp(&b.disk_total_bps),
                    SortColumn::RunTime => a.max_run_time_secs.cmp(&b.max_run_time_secs),
                };
                if self.sort_desc {
                    ord.reverse()
                } else {
                    ord
                }
            });

            let mut rows = Vec::with_capacity(groups.len() + 16);
            for g in groups {
                let is_expanded = self.expanded_groups.contains(&g.group_key);
                let children = g.children.clone();
                let multi = children.len() > 1;
                rows.push(DisplayRow::Group(g));
                if is_expanded && multi {
                    for child in children {
                        rows.push(DisplayRow::Child(child));
                    }
                }
            }
            rows
        } else {
            let mut list: Vec<ProcessEntry> = self
                .monitor
                .processes
                .iter()
                .filter(|p| {
                    if query.is_empty() {
                        return true;
                    }
                    p.name.to_lowercase().contains(&query)
                        || p.display_name.to_lowercase().contains(&query)
                        || p.pid.to_string().contains(&query)
                        || p.exe_path.to_lowercase().contains(&query)
                })
                .cloned()
                .collect();

            list.sort_by(|a, b| {
                if self.apps_first && a.is_user_app != b.is_user_app {
                    return b.is_user_app.cmp(&a.is_user_app);
                }
                let ord = match self.sort_col {
                    SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    SortColumn::Pid => a.pid.cmp(&b.pid),
                    SortColumn::Category => a.is_user_app.cmp(&b.is_user_app),
                    SortColumn::Cpu => a
                        .cpu_pct
                        .partial_cmp(&b.cpu_pct)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    SortColumn::Memory => a.memory_bytes.cmp(&b.memory_bytes),
                    SortColumn::Disk => a.disk_total_bps.cmp(&b.disk_total_bps),
                    SortColumn::RunTime => a.run_time_secs.cmp(&b.run_time_secs),
                };
                if self.sort_desc {
                    ord.reverse()
                } else {
                    ord
                }
            });

            list.into_iter().map(DisplayRow::Flat).collect()
        }
    }

    fn filtered_and_sorted_flat_processes(&self) -> Vec<ProcessEntry> {
        let query = self.search_query.trim().to_lowercase();
        let mut list: Vec<ProcessEntry> = self
            .monitor
            .processes
            .iter()
            .filter(|p| {
                if query.is_empty() {
                    return true;
                }
                p.name.to_lowercase().contains(&query)
                    || p.display_name.to_lowercase().contains(&query)
                    || p.pid.to_string().contains(&query)
                    || p.exe_path.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();

        list.sort_by(|a, b| {
            let ord = match self.sort_col {
                SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortColumn::Pid => a.pid.cmp(&b.pid),
                SortColumn::Category => a.is_user_app.cmp(&b.is_user_app),
                SortColumn::Cpu => a
                    .cpu_pct
                    .partial_cmp(&b.cpu_pct)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::Memory => a.memory_bytes.cmp(&b.memory_bytes),
                SortColumn::Disk => a.disk_total_bps.cmp(&b.disk_total_bps),
                SortColumn::RunTime => a.run_time_secs.cmp(&b.run_time_secs),
            };
            if self.sort_desc {
                ord.reverse()
            } else {
                ord
            }
        });
        list
    }

    fn trigger_kill_selected(&mut self, as_admin: bool) {
        if let Some(grp_key) = self.selected_group.clone() {
            match self.monitor.kill_group(&grp_key, as_admin) {
                Ok(msg) => {
                    self.selected_group = None;
                    self.selected_pid = None;
                    self.set_toast(msg, false);
                }
                Err(err) => self.set_toast(err, true),
            }
        } else if let Some(pid) = self.selected_pid {
            match self.monitor.kill_process(pid, true, as_admin) {
                Ok(msg) => {
                    self.selected_pid = None;
                    self.set_toast(msg, false);
                }
                Err(err) => self.set_toast(err, true),
            }
        }
    }

    fn render_top_bar(&mut self, ctx: &egui::Context, palette: &WinUiPalette, ctrl_held: bool) {
        egui::TopBottomPanel::top("winui_top_bar")
            .frame(
                Frame::none()
                    .fill(palette.bg_sidebar)
                    .inner_margin(Margin::symmetric(16.0, 10.0))
                    .stroke(Stroke::new(1.0_f32, palette.border_subtle)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;

                    // App icon + WinUI 3 Title
                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::hover());
                    ui.painter()
                        .rect_filled(icon_rect, Rounding::same(5.0), palette.accent);
                    ui.painter().line_segment(
                        [
                            Pos2::new(icon_rect.left() + 3.0, icon_rect.center().y + 2.0),
                            Pos2::new(icon_rect.left() + 7.5, icon_rect.center().y - 3.0),
                        ],
                        Stroke::new(1.8_f32, palette.accent_text_on_fill),
                    );
                    ui.painter().line_segment(
                        [
                            Pos2::new(icon_rect.left() + 7.5, icon_rect.center().y - 3.0),
                            Pos2::new(icon_rect.left() + 12.0, icon_rect.center().y + 3.0),
                        ],
                        Stroke::new(1.8_f32, palette.accent_text_on_fill),
                    );
                    ui.painter().line_segment(
                        [
                            Pos2::new(icon_rect.left() + 12.0, icon_rect.center().y + 3.0),
                            Pos2::new(icon_rect.right() - 3.0, icon_rect.center().y - 4.0),
                        ],
                        Stroke::new(1.8_f32, palette.accent_text_on_fill),
                    );

                    ui.label(
                        RichText::new("RigMap")
                            .size(14.0)
                            .strong()
                            .color(palette.text_primary),
                    );

                    if self.monitor.is_elevated {
                        Frame::none()
                            .fill(palette.accent_subtle_bg)
                            .rounding(Rounding::same(4.0))
                            .inner_margin(Margin::symmetric(6.0, 2.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("🛡 Admin")
                                        .size(11.0)
                                        .strong()
                                        .color(palette.accent),
                                );
                            });
                    }

                    ui.add_space(12.0);

                    // Centered WinUI 3 Search Box
                    let search_width = (ui.available_width() - 460.0).clamp(160.0, 360.0);
                    ui.add_sized(
                        [search_width, 28.0],
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("🔍  Search apps (e.g. Chrome, WhatsApp) or PID"),
                    );
                    if !self.search_query.is_empty() && ui.small_button("✕").clicked() {
                        self.search_query.clear();
                    }

                    // Hold Ctrl Freeze indicator badge
                    if ctrl_held {
                        Frame::none()
                            .fill(palette.bg_card_selected)
                            .rounding(Rounding::same(5.0))
                            .stroke(Stroke::new(1.0_f32, palette.warning))
                            .inner_margin(Margin::symmetric(8.0, 4.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("⏸ Ctrl Held: List Frozen")
                                        .size(11.5)
                                        .strong()
                                        .color(palette.warning),
                                );
                            });
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;

                        let has_sel =
                            self.selected_pid.is_some() || self.selected_group.is_some();

                        // 1. Administrator End Task Button
                        ui.add_enabled_ui(has_sel, |ui| {
                            let admin_btn = egui::Button::new(
                                RichText::new("🛡 End task (Admin)")
                                    .size(12.0)
                                    .strong()
                                    .color(if has_sel {
                                        palette.danger
                                    } else {
                                        palette.text_muted
                                    }),
                            )
                            .fill(palette.bg_card)
                            .rounding(Rounding::same(5.0));

                            if ui
                                .add(admin_btn)
                                .on_hover_text(
                                    "Force terminate process tree with Administrator privileges (UAC)",
                                )
                                .clicked()
                            {
                                self.trigger_kill_selected(true);
                            }
                        });

                        // 2. Standard End Task Accent Button
                        ui.add_enabled_ui(has_sel, |ui| {
                            let btn = egui::Button::new(
                                RichText::new("End task")
                                    .size(12.5)
                                    .strong()
                                    .color(if has_sel {
                                        palette.accent_text_on_fill
                                    } else {
                                        palette.text_muted
                                    }),
                            )
                            .fill(if has_sel {
                                palette.accent
                            } else {
                                palette.bg_card
                            })
                            .rounding(Rounding::same(5.0));

                            if ui.add(btn).clicked() {
                                self.trigger_kill_selected(false);
                            }
                        });

                        // 3. Run new task button
                        if ui
                            .button(RichText::new("➕ Run new task").size(12.0))
                            .clicked()
                        {
                            self.show_run_task_modal = true;
                        }

                        // 4. Elevate RigMap as Admin button (if not already elevated)
                        if !self.monitor.is_elevated
                            && ui
                                .button(RichText::new("🛡 Restart as Admin").size(12.0))
                                .on_hover_text("Relaunch RigMap with full Administrator privileges")
                                .clicked()
                        {
                            if let Err(e) = SystemMonitor::restart_self_as_admin() {
                                self.set_toast(e, true);
                            }
                        }
                    });
                });
            });
    }

    fn render_sidebar(&mut self, ctx: &egui::Context, palette: &WinUiPalette) {
        let width = if self.sidebar_expanded { 205.0 } else { 56.0 };

        egui::SidePanel::left("winui_nav_sidebar")
            .exact_width(width)
            .resizable(false)
            .frame(
                Frame::none()
                    .fill(palette.bg_sidebar)
                    .inner_margin(Margin::symmetric(8.0, 10.0))
                    .stroke(Stroke::new(1.0_f32, palette.border_subtle)),
            )
            .show(ctx, |ui| {
                if ui
                    .add_sized(
                        [40.0, 34.0],
                        egui::Button::new(RichText::new("☰").size(16.0)).frame(false),
                    )
                    .on_hover_text("Toggle Navigation Pane")
                    .clicked()
                {
                    self.sidebar_expanded = !self.sidebar_expanded;
                }

                ui.add_space(6.0);

                let nav_items = [
                    (NavPage::Processes, "⊞", "Processes"),
                    (NavPage::Performance, "📈", "Performance"),
                    (NavPage::ThisPc, "💻", "This PC"),
                    (NavPage::StartupApps, "🚀", "Startup apps"),
                    (NavPage::Users, "👤", "Users"),
                    (NavPage::Details, "☰", "Details"),
                ];

                for (page, icon, label) in nav_items {
                    self.draw_nav_item(ui, palette, page, icon, label);
                    ui.add_space(3.0);
                }

                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.add_space(4.0);
                    self.draw_nav_item(ui, palette, NavPage::Settings, "⚙", "Settings");
                    ui.add_space(4.0);
                    ui.separator();
                });
            });
    }

    fn draw_nav_item(
        &mut self,
        ui: &mut egui::Ui,
        palette: &WinUiPalette,
        page: NavPage,
        icon: &str,
        label: &str,
    ) {
        let selected = self.nav_page == page;
        let desired_size = Vec2::new(ui.available_width(), 38.0);
        let (rect, resp) = ui.allocate_exact_size(desired_size, Sense::click());

        if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }

        if selected || resp.hovered() {
            let fill = if selected {
                palette.bg_card_selected
            } else {
                palette.bg_card_hover
            };
            ui.painter().rect_filled(rect, Rounding::same(6.0), fill);
        }

        if selected {
            let pill_rect = Rect::from_min_size(
                Pos2::new(rect.left() + 3.0, rect.center().y - 8.0),
                Vec2::new(3.5, 16.0),
            );
            ui.painter()
                .rect_filled(pill_rect, Rounding::same(2.0), palette.accent);
        }

        let text_color = if selected {
            palette.text_primary
        } else {
            palette.text_secondary
        };

        ui.painter().text(
            Pos2::new(rect.left() + 16.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            icon,
            egui::FontId::proportional(15.0),
            if selected { palette.accent } else { text_color },
        );

        if self.sidebar_expanded {
            ui.painter().text(
                Pos2::new(rect.left() + 44.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(13.5),
                text_color,
            );
        }

        if resp.clicked() {
            self.nav_page = page;
        }
    }

    fn render_processes_page(&mut self, ui: &mut egui::Ui, palette: &WinUiPalette) {
        let rows = self.build_process_rows();
        let app_count = self
            .monitor
            .app_groups
            .iter()
            .filter(|g| g.is_user_app)
            .count();

        // Top KPI Header Strip with generous spacing
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.label(
                RichText::new("Processes")
                    .size(21.0)
                    .strong()
                    .color(palette.text_primary),
            );
            ui.label(
                RichText::new(format!(
                    "({} Apps · {} Total processes)",
                    app_count,
                    self.monitor.processes.len()
                ))
                .size(12.5)
                .color(palette.text_secondary),
            );

            ui.add_space(8.0);
            ui.checkbox(&mut self.group_by_app, "Group by app");
            ui.checkbox(&mut self.apps_first, "Apps on top");

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                self.draw_kpi_pill(
                    ui,
                    palette,
                    "Network",
                    &format!(
                        "↓ {:.0} Kbps",
                        (self.monitor.net_rx_bps as f32 * 8.0) / 1000.0
                    ),
                    palette.chart_net,
                );
                self.draw_kpi_pill(
                    ui,
                    palette,
                    "Disk",
                    &format_rate_bps(self.monitor.disk_read_bps + self.monitor.disk_write_bps),
                    palette.chart_disk,
                );
                self.draw_kpi_pill(
                    ui,
                    palette,
                    "Memory",
                    &format!("{:.0}%", self.monitor.mem_usage_pct),
                    palette.chart_mem,
                );
                self.draw_kpi_pill(
                    ui,
                    palette,
                    "CPU",
                    &format!("{:.0}%", self.monitor.cpu_usage_total),
                    palette.chart_cpu,
                );
            });
        });

        ui.add_space(12.0);

        // WinUI 3 Card Container for Process Table with clean inner padding
        Frame::none()
            .fill(palette.bg_card)
            .rounding(Rounding::same(8.0))
            .stroke(Stroke::new(1.0_f32, palette.border_card))
            .inner_margin(Margin::symmetric(12.0, 10.0))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(12.0, 4.0);
                let avail_h = ui.available_height().max(200.0);

                TableBuilder::new(ui)
                    .striped(false)
                    .resizable(true)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .min_scrolled_height(avail_h)
                    .max_scroll_height(avail_h)
                    .column(Column::initial(260.0).at_least(160.0).clip(true)) // Name
                    .column(Column::initial(80.0).at_least(65.0)) // PID
                    .column(Column::initial(110.0).at_least(85.0)) // Type (App / Background)
                    .column(Column::initial(105.0).at_least(85.0)) // CPU
                    .column(Column::initial(130.0).at_least(100.0)) // Memory
                    .column(Column::initial(115.0).at_least(90.0)) // Disk
                    .column(Column::remainder().at_least(95.0)) // Uptime
                    .header(36.0, |mut header| {
                        let cols = [
                            (SortColumn::Name, "Name".to_string()),
                            (SortColumn::Pid, "PID".to_string()),
                            (SortColumn::Category, "Category".to_string()),
                            (
                                SortColumn::Cpu,
                                format!("CPU ({:.0}%)", self.monitor.cpu_usage_total),
                            ),
                            (
                                SortColumn::Memory,
                                format!("Memory ({:.0}%)", self.monitor.mem_usage_pct),
                            ),
                            (SortColumn::Disk, "Disk".to_string()),
                            (SortColumn::RunTime, "Run time".to_string()),
                        ];

                        for (col_id, title) in cols {
                            header.col(|ui| {
                                ui.add_space(4.0);
                                let is_sorted = self.sort_col == col_id;
                                let arrow = if is_sorted {
                                    if self.sort_desc {
                                        " ▼"
                                    } else {
                                        " ▲"
                                    }
                                } else {
                                    ""
                                };
                                let label_text = format!("{}{}", title, arrow);
                                let resp = ui.add(
                                    egui::Button::new(
                                        RichText::new(label_text).size(12.5).strong().color(
                                            if is_sorted {
                                                palette.accent
                                            } else {
                                                palette.text_secondary
                                            },
                                        ),
                                    )
                                    .frame(false),
                                );
                                if resp.clicked() {
                                    if self.sort_col == col_id {
                                        self.sort_desc = !self.sort_desc;
                                    } else {
                                        self.sort_col = col_id;
                                        self.sort_desc = true;
                                    }
                                }
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(32.0, rows.len(), |mut row| {
                            let item = &rows[row.index()];
                            match item {
                                DisplayRow::Group(g) => {
                                    let is_selected =
                                        self.selected_group.as_deref() == Some(&g.group_key);
                                    row.set_selected(is_selected);
                                    let count = g.children.len();
                                    let is_expanded = self.expanded_groups.contains(&g.group_key);

                                    // 1. Name (with expand chevron for multi-process apps like Chrome & WhatsApp)
                                    row.col(|ui| {
                                        ui.add_space(4.0);
                                        if count > 1 {
                                            let chev = if is_expanded { "▼" } else { "▶" };
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new(chev)
                                                            .size(10.5)
                                                            .color(palette.text_secondary),
                                                    )
                                                    .frame(false),
                                                )
                                                .clicked()
                                            {
                                                if is_expanded {
                                                    self.expanded_groups.remove(&g.group_key);
                                                } else {
                                                    self.expanded_groups
                                                        .insert(g.group_key.clone());
                                                }
                                            }
                                        } else {
                                            ui.add_space(16.0);
                                        }

                                        let title = if count > 1 {
                                            format!("{} ({})", g.display_name, count)
                                        } else {
                                            g.display_name.clone()
                                        };
                                        let resp = ui.add(
                                            egui::Label::new(
                                                RichText::new(title)
                                                    .size(13.0)
                                                    .strong()
                                                    .color(palette.text_primary),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        self.handle_group_interactions(ui, resp, g);
                                    });

                                    // 2. Primary PID
                                    row.col(|ui| {
                                        ui.add_space(4.0);
                                        let resp = ui.add(
                                            egui::Label::new(
                                                RichText::new(g.primary_pid.to_string())
                                                    .monospace()
                                                    .size(12.5)
                                                    .color(palette.text_secondary),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        self.handle_group_interactions(ui, resp, g);
                                    });

                                    // 3. Category Badge (App vs Background)
                                    row.col(|ui| {
                                        ui.add_space(4.0);
                                        let (tag, col) = if g.is_user_app {
                                            ("App", palette.accent)
                                        } else {
                                            ("Background", palette.text_muted)
                                        };
                                        let resp = ui.add(
                                            egui::Label::new(
                                                RichText::new(tag).size(12.0).strong().color(col),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        self.handle_group_interactions(ui, resp, g);
                                    });

                                    // 4. CPU % (Heatmap cell shrunk so cells don't touch side-by-side)
                                    row.col(|ui| {
                                        if self.show_heatmap && !is_selected {
                                            let heat = (g.cpu_pct / 35.0).clamp(0.05, 1.0);
                                            let cell_rect =
                                                ui.max_rect().shrink2(Vec2::new(4.0, 2.5));
                                            ui.painter().rect_filled(
                                                cell_rect,
                                                Rounding::same(4.0),
                                                palette.heatmap_cell_bg(heat, self.theme_mode),
                                            );
                                        }
                                        ui.add_space(8.0);
                                        let resp = ui.add(
                                            egui::Label::new(
                                                RichText::new(format!("{:.1}%", g.cpu_pct))
                                                    .monospace()
                                                    .size(12.5)
                                                    .color(palette.text_primary),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        self.handle_group_interactions(ui, resp, g);
                                    });

                                    // 5. Memory
                                    row.col(|ui| {
                                        if self.show_heatmap && !is_selected {
                                            let heat = (g.memory_pct / 15.0).clamp(0.05, 1.0);
                                            let cell_rect =
                                                ui.max_rect().shrink2(Vec2::new(4.0, 2.5));
                                            ui.painter().rect_filled(
                                                cell_rect,
                                                Rounding::same(4.0),
                                                palette.heatmap_cell_bg(heat, self.theme_mode),
                                            );
                                        }
                                        ui.add_space(8.0);
                                        let resp = ui.add(
                                            egui::Label::new(
                                                RichText::new(format_bytes(g.memory_bytes))
                                                    .monospace()
                                                    .size(12.5)
                                                    .color(palette.text_primary),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        self.handle_group_interactions(ui, resp, g);
                                    });

                                    // 6. Disk
                                    row.col(|ui| {
                                        if self.show_heatmap && !is_selected && g.disk_total_bps > 0
                                        {
                                            let heat = (g.disk_total_bps as f32
                                                / (10.0 * 1024.0 * 1024.0))
                                                .clamp(0.15, 1.0);
                                            let cell_rect =
                                                ui.max_rect().shrink2(Vec2::new(4.0, 2.5));
                                            ui.painter().rect_filled(
                                                cell_rect,
                                                Rounding::same(4.0),
                                                palette.heatmap_cell_bg(heat, self.theme_mode),
                                            );
                                        }
                                        ui.add_space(8.0);
                                        let resp = ui.add(
                                            egui::Label::new(
                                                RichText::new(format_rate_bps(g.disk_total_bps))
                                                    .monospace()
                                                    .size(12.5)
                                                    .color(palette.text_secondary),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        self.handle_group_interactions(ui, resp, g);
                                    });

                                    // 7. Run Time
                                    row.col(|ui| {
                                        ui.add_space(6.0);
                                        let resp = ui.add(
                                            egui::Label::new(
                                                RichText::new(format_duration(g.max_run_time_secs))
                                                    .monospace()
                                                    .size(12.0)
                                                    .color(palette.text_secondary),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        self.handle_group_interactions(ui, resp, g);
                                    });
                                }
                                DisplayRow::Child(p) => {
                                    self.draw_process_row(&mut row, palette, p, true);
                                }
                                DisplayRow::Flat(p) => {
                                    self.draw_process_row(&mut row, palette, p, false);
                                }
                            }
                        });
                    });
            });
    }

    fn draw_process_row(
        &mut self,
        row: &mut egui_extras::TableRow<'_, '_>,
        palette: &WinUiPalette,
        p: &ProcessEntry,
        indented: bool,
    ) {
        let is_selected = self.selected_pid == Some(p.pid) && self.selected_group.is_none();
        row.set_selected(is_selected);

        row.col(|ui| {
            ui.add_space(if indented { 32.0 } else { 8.0 });
            let label = if indented {
                format!("↳ {}", p.name)
            } else {
                format!("{} ({})", p.display_name, p.name)
            };
            let resp = ui.add(
                egui::Label::new(RichText::new(label).size(12.5).color(if indented {
                    palette.text_secondary
                } else {
                    palette.text_primary
                }))
                .sense(Sense::click()),
            );
            self.handle_row_interactions(ui, resp, p);
        });

        row.col(|ui| {
            ui.add_space(4.0);
            let resp = ui.add(
                egui::Label::new(
                    RichText::new(p.pid.to_string())
                        .monospace()
                        .size(12.0)
                        .color(palette.text_secondary),
                )
                .sense(Sense::click()),
            );
            self.handle_row_interactions(ui, resp, p);
        });

        row.col(|ui| {
            ui.add_space(4.0);
            let (tag, col) = if p.is_user_app {
                ("App", palette.accent)
            } else {
                ("Background", palette.text_muted)
            };
            let resp = ui.add(
                egui::Label::new(RichText::new(tag).size(12.0).color(col)).sense(Sense::click()),
            );
            self.handle_row_interactions(ui, resp, p);
        });

        row.col(|ui| {
            if self.show_heatmap && !is_selected {
                let heat = (p.cpu_pct / 35.0).clamp(0.05, 1.0);
                let cell_rect = ui.max_rect().shrink2(Vec2::new(4.0, 2.5));
                ui.painter().rect_filled(
                    cell_rect,
                    Rounding::same(4.0),
                    palette.heatmap_cell_bg(heat, self.theme_mode),
                );
            }
            ui.add_space(8.0);
            let resp = ui.add(
                egui::Label::new(
                    RichText::new(format!("{:.1}%", p.cpu_pct))
                        .monospace()
                        .size(12.0)
                        .color(palette.text_primary),
                )
                .sense(Sense::click()),
            );
            self.handle_row_interactions(ui, resp, p);
        });

        row.col(|ui| {
            if self.show_heatmap && !is_selected {
                let heat = (p.memory_pct / 15.0).clamp(0.05, 1.0);
                let cell_rect = ui.max_rect().shrink2(Vec2::new(4.0, 2.5));
                ui.painter().rect_filled(
                    cell_rect,
                    Rounding::same(4.0),
                    palette.heatmap_cell_bg(heat, self.theme_mode),
                );
            }
            ui.add_space(8.0);
            let resp = ui.add(
                egui::Label::new(
                    RichText::new(format_bytes(p.memory_bytes))
                        .monospace()
                        .size(12.0)
                        .color(palette.text_primary),
                )
                .sense(Sense::click()),
            );
            self.handle_row_interactions(ui, resp, p);
        });

        row.col(|ui| {
            ui.add_space(8.0);
            let resp = ui.add(
                egui::Label::new(
                    RichText::new(format_rate_bps(p.disk_total_bps))
                        .monospace()
                        .size(12.0)
                        .color(palette.text_secondary),
                )
                .sense(Sense::click()),
            );
            self.handle_row_interactions(ui, resp, p);
        });

        row.col(|ui| {
            ui.add_space(6.0);
            let resp = ui.add(
                egui::Label::new(
                    RichText::new(format_duration(p.run_time_secs))
                        .monospace()
                        .size(12.0)
                        .color(palette.text_secondary),
                )
                .sense(Sense::click()),
            );
            self.handle_row_interactions(ui, resp, p);
        });
    }

    fn handle_group_interactions(
        &mut self,
        _ui: &mut egui::Ui,
        resp: egui::Response,
        g: &AppGroupEntry,
    ) {
        if resp.clicked() {
            self.selected_group = Some(g.group_key.clone());
            self.selected_pid = Some(g.primary_pid);
        }
        if resp.double_clicked() && g.children.len() > 1 {
            if self.expanded_groups.contains(&g.group_key) {
                self.expanded_groups.remove(&g.group_key);
            } else {
                self.expanded_groups.insert(g.group_key.clone());
            }
        }
        resp.context_menu(|ui| {
            self.selected_group = Some(g.group_key.clone());
            self.selected_pid = Some(g.primary_pid);
            ui.label(
                RichText::new(format!("{} ({} processes)", g.display_name, g.children.len()))
                    .strong(),
            );
            ui.separator();
            if ui.button("⏹  End task").clicked() {
                self.trigger_kill_selected(false);
                ui.close_menu();
            }
            if ui.button("🛡  End task as Administrator").clicked() {
                self.trigger_kill_selected(true);
                ui.close_menu();
            }
            if g.children.len() > 1 {
                let expand_txt = if self.expanded_groups.contains(&g.group_key) {
                    "🔼  Collapse group"
                } else {
                    "🔽  Expand child processes"
                };
                if ui.button(expand_txt).clicked() {
                    if self.expanded_groups.contains(&g.group_key) {
                        self.expanded_groups.remove(&g.group_key);
                    } else {
                        self.expanded_groups.insert(g.group_key.clone());
                    }
                    ui.close_menu();
                }
            }
            if !g.exe_path.is_empty() && ui.button("📂  Open file location").clicked() {
                SystemMonitor::open_file_location(&g.exe_path);
                ui.close_menu();
            }
            if ui.button("📋  Copy Primary PID").clicked() {
                ui.output_mut(|o| o.copied_text = g.primary_pid.to_string());
                ui.close_menu();
            }
            if ui.button("🔎  Go to details").clicked() {
                self.nav_page = NavPage::Details;
                ui.close_menu();
            }
        });
    }

    fn handle_row_interactions(
        &mut self,
        _ui: &mut egui::Ui,
        resp: egui::Response,
        p: &ProcessEntry,
    ) {
        if resp.clicked() {
            self.selected_group = None;
            self.selected_pid = Some(p.pid);
        }
        resp.context_menu(|ui| {
            self.selected_group = None;
            self.selected_pid = Some(p.pid);
            ui.label(RichText::new(format!("{} (PID {})", p.name, p.pid)).strong());
            ui.separator();
            if ui.button("⏹  End task").clicked() {
                self.trigger_kill_selected(false);
                ui.close_menu();
            }
            if ui.button("🛡  End task as Administrator").clicked() {
                self.trigger_kill_selected(true);
                ui.close_menu();
            }
            if !p.exe_path.is_empty() && ui.button("📂  Open file location").clicked() {
                SystemMonitor::open_file_location(&p.exe_path);
                ui.close_menu();
            }
            if ui.button("📋  Copy PID").clicked() {
                ui.output_mut(|o| o.copied_text = p.pid.to_string());
                ui.close_menu();
            }
            if ui.button("🔎  Go to details").clicked() {
                self.nav_page = NavPage::Details;
                ui.close_menu();
            }
        });
    }

    fn draw_kpi_pill(
        &self,
        ui: &mut egui::Ui,
        palette: &WinUiPalette,
        label: &str,
        value: &str,
        dot_color: Color32,
    ) {
        Frame::none()
            .fill(palette.bg_card)
            .rounding(Rounding::same(6.0))
            .stroke(Stroke::new(1.0_f32, palette.border_card))
            .inner_margin(Margin::symmetric(12.0, 5.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let (dot_rect, _) =
                        ui.allocate_exact_size(Vec2::new(8.0, 8.0), Sense::hover());
                    ui.painter()
                        .circle_filled(dot_rect.center(), 4.0, dot_color);
                    ui.label(
                        RichText::new(format!("{}:", label))
                            .size(12.0)
                            .color(palette.text_secondary),
                    );
                    ui.label(
                        RichText::new(value)
                            .size(12.5)
                            .strong()
                            .color(palette.text_primary),
                    );
                });
            });
    }

    fn render_performance_split(&mut self, ui: &mut egui::Ui, palette: &WinUiPalette) {
        ui.label(
            RichText::new("Performance")
                .size(21.0)
                .strong()
                .color(palette.text_primary),
        );
        ui.add_space(12.0);

        egui::SidePanel::left("perf_resource_selector")
            .exact_width(260.0)
            .resizable(false)
            .frame(Frame::none().inner_margin(Margin {
                left: 0.0,
                right: 14.0,
                top: 0.0,
                bottom: 0.0,
            }))
            .show_inside(ui, |ui| {
                let cpu_hist = self.monitor.cpu_history.clone();
                let mem_hist = self.monitor.mem_history.clone();
                let disk_hist = self.monitor.disk_read_history.clone();
                let net_hist = self.monitor.net_rx_history.clone();

                self.draw_perf_selector_card(
                    ui,
                    palette,
                    PerfResource::Cpu,
                    "CPU",
                    &format!(
                        "{:.0}%   {:.2} GHz",
                        self.monitor.cpu_usage_total, self.monitor.cpu_freq_ghz
                    ),
                    &cpu_hist,
                    100.0,
                    palette.chart_cpu,
                );
                ui.add_space(8.0);

                self.draw_perf_selector_card(
                    ui,
                    palette,
                    PerfResource::Memory,
                    "Memory",
                    &format!(
                        "{:.1}/{:.1} GB ({:.0}%)",
                        self.monitor.mem_used_bytes as f64 / 1_073_741_824.0,
                        self.monitor.mem_total_bytes as f64 / 1_073_741_824.0,
                        self.monitor.mem_usage_pct
                    ),
                    &mem_hist,
                    100.0,
                    palette.chart_mem,
                );
                ui.add_space(8.0);

                self.draw_perf_selector_card(
                    ui,
                    palette,
                    PerfResource::Disk,
                    "Disk",
                    &format_rate_bps(self.monitor.disk_read_bps + self.monitor.disk_write_bps),
                    &disk_hist,
                    25.0,
                    palette.chart_disk,
                );
                ui.add_space(8.0);

                self.draw_perf_selector_card(
                    ui,
                    palette,
                    PerfResource::Network,
                    "Network",
                    &format!(
                        "↓ {:.0}  ↑ {:.0} Kbps",
                        (self.monitor.net_rx_bps as f32 * 8.0) / 1000.0,
                        (self.monitor.net_tx_bps as f32 * 8.0) / 1000.0
                    ),
                    &net_hist,
                    500.0,
                    palette.chart_net,
                );
            });

        egui::CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(palette.bg_card)
                    .rounding(Rounding::same(8.0))
                    .stroke(Stroke::new(1.0_f32, palette.border_card))
                    .inner_margin(Margin::same(18.0)),
            )
            .show_inside(ui, |ui| match self.perf_resource {
                PerfResource::Cpu => self.render_perf_cpu(ui, palette),
                PerfResource::Memory => self.render_perf_memory(ui, palette),
                PerfResource::Disk => self.render_perf_disk(ui, palette),
                PerfResource::Network => self.render_perf_network(ui, palette),
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_perf_selector_card(
        &mut self,
        ui: &mut egui::Ui,
        palette: &WinUiPalette,
        res: PerfResource,
        title: &str,
        subtitle: &str,
        history: &VecDeque<f32>,
        max_y: f32,
        color: Color32,
    ) {
        let selected = self.perf_resource == res;
        let desired = Vec2::new(ui.available_width(), 72.0);
        let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());

        let bg = if selected {
            palette.bg_card_selected
        } else if resp.hovered() {
            palette.bg_card_hover
        } else {
            palette.bg_card
        };

        ui.painter().rect(
            rect,
            Rounding::same(7.0),
            bg,
            Stroke::new(
                1.0_f32,
                if selected {
                    palette.accent
                } else {
                    palette.border_card
                },
            ),
        );

        if selected {
            let pill = Rect::from_min_size(
                Pos2::new(rect.left() + 3.0, rect.center().y - 14.0),
                Vec2::new(3.5, 28.0),
            );
            ui.painter()
                .rect_filled(pill, Rounding::same(2.0), palette.accent);
        }

        let spark_rect = Rect::from_min_size(
            Pos2::new(rect.left() + 14.0, rect.top() + 14.0),
            Vec2::new(64.0, 44.0),
        );
        self.draw_mini_sparkline(ui, spark_rect, history, max_y, color, palette);

        ui.painter().text(
            Pos2::new(spark_rect.right() + 14.0, rect.top() + 24.0),
            egui::Align2::LEFT_CENTER,
            title,
            egui::FontId::proportional(14.5),
            palette.text_primary,
        );
        ui.painter().text(
            Pos2::new(spark_rect.right() + 14.0, rect.top() + 46.0),
            egui::Align2::LEFT_CENTER,
            subtitle,
            egui::FontId::proportional(12.0),
            palette.text_secondary,
        );

        if resp.clicked() {
            self.perf_resource = res;
        }
    }

    fn draw_mini_sparkline(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        history: &VecDeque<f32>,
        max_y: f32,
        color: Color32,
        palette: &WinUiPalette,
    ) {
        ui.painter().rect(
            rect,
            Rounding::same(4.0),
            palette.bg_subtle,
            Stroke::new(1.0_f32, palette.border_subtle),
        );

        if history.len() < 2 {
            return;
        }

        let effective_max = history.iter().copied().fold(max_y.max(1.0), f32::max);
        let n = history.len() as f32;
        let pts: Vec<Pos2> = history
            .iter()
            .enumerate()
            .map(|(i, &val)| {
                let x = rect.left() + (i as f32 / (n - 1.0)) * rect.width();
                let norm = (val / effective_max).clamp(0.0, 1.0);
                let y = rect.bottom() - 2.0 - norm * (rect.height() - 4.0);
                Pos2::new(x, y)
            })
            .collect();

        ui.painter()
            .add(egui::Shape::line(pts, Stroke::new(1.6_f32, color)));
    }

    fn render_perf_cpu(&mut self, ui: &mut egui::Ui, palette: &WinUiPalette) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("CPU")
                    .size(22.0)
                    .strong()
                    .color(palette.text_primary),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(&self.monitor.cpu_brand)
                        .size(14.0)
                        .color(palette.text_secondary),
                );
            });
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if self.cpu_per_core_view {
                    "% Utilization over 60 seconds (Logical processors)"
                } else {
                    "% Utilization over 60 seconds"
                })
                .size(12.0)
                .color(palette.text_muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let toggle_label = if self.cpu_per_core_view {
                    "Show overall utilization"
                } else {
                    "Show logical processors"
                };
                if ui.small_button(toggle_label).clicked() {
                    self.cpu_per_core_view = !self.cpu_per_core_view;
                }
            });
        });

        ui.add_space(8.0);

        let chart_height = (ui.available_height() - 150.0).max(160.0);

        if !self.cpu_per_core_view {
            let pts: PlotPoints = self
                .monitor
                .cpu_history
                .iter()
                .enumerate()
                .map(|(i, &v)| [i as f64, v as f64])
                .collect();

            Plot::new("cpu_main_plot")
                .height(chart_height)
                .include_y(0.0)
                .include_y(100.0)
                .include_x(0.0)
                .include_x(HISTORY_LEN as f64)
                .show_axes([false, true])
                .allow_drag(false)
                .allow_zoom(false)
                .allow_scroll(false)
                .show(ui, |plot_ui| {
                    plot_ui.line(
                        Line::new(pts)
                            .color(palette.chart_cpu)
                            .fill(0.0_f32)
                            .width(2.0_f32),
                    );
                });
        } else {
            egui::ScrollArea::vertical()
                .max_height(chart_height)
                .show(ui, |ui| {
                    let cols = 4.min(self.monitor.logical_cores.max(1));
                    egui::Grid::new("cpu_cores_grid")
                        .num_columns(cols)
                        .spacing([12.0, 12.0])
                        .show(ui, |ui| {
                            for (idx, hist) in self.monitor.per_core_history.iter().enumerate() {
                                let usage =
                                    self.monitor.per_core_usage.get(idx).copied().unwrap_or(0.0);
                                Frame::none()
                                    .fill(palette.bg_subtle)
                                    .rounding(Rounding::same(6.0))
                                    .stroke(Stroke::new(1.0_f32, palette.border_subtle))
                                    .inner_margin(Margin::same(10.0))
                                    .show(ui, |ui| {
                                        ui.set_min_width(135.0);
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(format!("CPU {}", idx))
                                                    .size(11.5)
                                                    .color(palette.text_secondary),
                                            );
                                            ui.with_layout(
                                                Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    ui.label(
                                                        RichText::new(format!("{:.0}%", usage))
                                                            .size(11.5)
                                                            .strong(),
                                                    );
                                                },
                                            );
                                        });
                                        let (rect, _) = ui.allocate_exact_size(
                                            Vec2::new(135.0, 46.0),
                                            Sense::hover(),
                                        );
                                        self.draw_mini_sparkline(
                                            ui,
                                            rect,
                                            hist,
                                            100.0,
                                            palette.chart_cpu,
                                            palette,
                                        );
                                    });

                                if (idx + 1) % cols == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                });
        }

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(10.0);

        ui.horizontal_wrapped(|ui| {
            self.draw_stat_block(
                ui,
                palette,
                "Utilization",
                &format!("{:.0}%", self.monitor.cpu_usage_total),
            );
            self.draw_stat_block(
                ui,
                palette,
                "Speed",
                &format!("{:.2} GHz", self.monitor.cpu_freq_ghz),
            );
            self.draw_stat_block(
                ui,
                palette,
                "Processes",
                &self.monitor.processes.len().to_string(),
            );
            self.draw_stat_block(
                ui,
                palette,
                "Cores",
                &format!(
                    "{} Physical / {} Logical",
                    self.monitor.physical_cores, self.monitor.logical_cores
                ),
            );
            self.draw_stat_block(
                ui,
                palette,
                "Up time",
                &format_duration(self.monitor.uptime_secs),
            );
        });
    }

    fn render_perf_memory(&mut self, ui: &mut egui::Ui, palette: &WinUiPalette) {
        let total_gb = self.monitor.mem_total_bytes as f64 / 1_073_741_824.0;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Memory")
                    .size(22.0)
                    .strong()
                    .color(palette.text_primary),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{:.1} GB", total_gb))
                        .size(16.0)
                        .color(palette.text_secondary),
                );
            });
        });

        ui.label(
            RichText::new("Memory usage over 60 seconds")
                .size(12.0)
                .color(palette.text_muted),
        );
        ui.add_space(8.0);

        let chart_height = (ui.available_height() - 190.0).max(150.0);
        let pts: PlotPoints = self
            .monitor
            .mem_history
            .iter()
            .enumerate()
            .map(|(i, &v)| [i as f64, v as f64])
            .collect();

        Plot::new("mem_main_plot")
            .height(chart_height)
            .include_y(0.0)
            .include_y(100.0)
            .include_x(0.0)
            .include_x(HISTORY_LEN as f64)
            .show_axes([false, true])
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .show(ui, |plot_ui| {
                plot_ui.line(
                    Line::new(pts)
                        .color(palette.chart_mem)
                        .fill(0.0_f32)
                        .width(2.0_f32),
                );
            });

        ui.add_space(10.0);
        ui.label(
            RichText::new("Memory composition")
                .size(12.0)
                .color(palette.text_muted),
        );
        let (bar_rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
        ui.painter()
            .rect_filled(bar_rect, Rounding::same(4.0), palette.bg_subtle);
        let used_w = bar_rect.width() * (self.monitor.mem_usage_pct / 100.0).clamp(0.0, 1.0);
        let used_rect = Rect::from_min_size(bar_rect.min, Vec2::new(used_w, bar_rect.height()));
        ui.painter()
            .rect_filled(used_rect, Rounding::same(4.0), palette.chart_mem);

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        ui.horizontal_wrapped(|ui| {
            self.draw_stat_block(
                ui,
                palette,
                "In use",
                &format_bytes(self.monitor.mem_used_bytes),
            );
            self.draw_stat_block(
                ui,
                palette,
                "Available",
                &format_bytes(self.monitor.mem_available_bytes),
            );
            self.draw_stat_block(
                ui,
                palette,
                "Committed (Swap)",
                &format!(
                    "{} / {}",
                    format_bytes(self.monitor.swap_used_bytes),
                    format_bytes(self.monitor.swap_total_bytes)
                ),
            );
            self.draw_stat_block(
                ui,
                palette,
                "Utilization",
                &format!("{:.1}%", self.monitor.mem_usage_pct),
            );
        });
    }

    fn render_perf_disk(&mut self, ui: &mut egui::Ui, palette: &WinUiPalette) {
        ui.label(
            RichText::new("Disk Transfer & Volumes")
                .size(22.0)
                .strong()
                .color(palette.text_primary),
        );
        ui.label(
            RichText::new("Active Read / Write throughput (MB/s)")
                .size(12.0)
                .color(palette.text_muted),
        );
        ui.add_space(8.0);

        let read_pts: PlotPoints = self
            .monitor
            .disk_read_history
            .iter()
            .enumerate()
            .map(|(i, &v)| [i as f64, v as f64])
            .collect();
        let write_pts: PlotPoints = self
            .monitor
            .disk_write_history
            .iter()
            .enumerate()
            .map(|(i, &v)| [i as f64, v as f64])
            .collect();

        Plot::new("disk_main_plot")
            .height(175.0)
            .include_y(0.0)
            .include_y(10.0)
            .include_x(0.0)
            .include_x(HISTORY_LEN as f64)
            .show_axes([false, true])
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .show(ui, |plot_ui| {
                plot_ui.line(
                    Line::new(read_pts)
                        .name("Read MB/s")
                        .color(palette.chart_disk)
                        .fill(0.0_f32)
                        .width(2.0_f32),
                );
                plot_ui.line(
                    Line::new(write_pts)
                        .name("Write MB/s")
                        .color(palette.chart_cpu)
                        .width(1.8_f32),
                );
            });

        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            self.draw_stat_block(
                ui,
                palette,
                "Read speed",
                &format_rate_bps(self.monitor.disk_read_bps),
            );
            self.draw_stat_block(
                ui,
                palette,
                "Write speed",
                &format_rate_bps(self.monitor.disk_write_bps),
            );
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            for d in &self.monitor.disk_list {
                Frame::none()
                    .fill(palette.bg_subtle)
                    .rounding(Rounding::same(6.0))
                    .stroke(Stroke::new(1.0_f32, palette.border_subtle))
                    .inner_margin(Margin::same(12.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("🖴  {} ({})", d.mount_point, d.name))
                                    .size(13.5)
                                    .strong(),
                            );
                            ui.label(
                                RichText::new(format!("[{} - {}]", d.file_system, d.kind))
                                    .size(12.0)
                                    .color(palette.text_muted),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} free of {} ({:.1}% used)",
                                        format_bytes(d.available_bytes),
                                        format_bytes(d.total_bytes),
                                        d.used_pct
                                    ))
                                    .size(12.5)
                                    .color(palette.text_secondary),
                                );
                            });
                        });

                        ui.add_space(4.0);
                        let (bar_rect, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 8.0),
                            Sense::hover(),
                        );
                        ui.painter()
                            .rect_filled(bar_rect, Rounding::same(4.0), palette.bg_card);
                        let fill_w = bar_rect.width() * (d.used_pct / 100.0).clamp(0.0, 1.0);
                        ui.painter().rect_filled(
                            Rect::from_min_size(bar_rect.min, Vec2::new(fill_w, bar_rect.height())),
                            Rounding::same(4.0),
                            palette.chart_disk,
                        );
                    });
                ui.add_space(8.0);
            }
        });
    }

    fn render_perf_network(&mut self, ui: &mut egui::Ui, palette: &WinUiPalette) {
        ui.label(
            RichText::new("Network Throughput")
                .size(22.0)
                .strong()
                .color(palette.text_primary),
        );
        ui.label(
            RichText::new("Throughput over 60 seconds (Kbps)")
                .size(12.0)
                .color(palette.text_muted),
        );
        ui.add_space(8.0);

        let rx_pts: PlotPoints = self
            .monitor
            .net_rx_history
            .iter()
            .enumerate()
            .map(|(i, &v)| [i as f64, v as f64])
            .collect();
        let tx_pts: PlotPoints = self
            .monitor
            .net_tx_history
            .iter()
            .enumerate()
            .map(|(i, &v)| [i as f64, v as f64])
            .collect();

        Plot::new("net_main_plot")
            .height(180.0)
            .include_y(0.0)
            .include_y(100.0)
            .include_x(0.0)
            .include_x(HISTORY_LEN as f64)
            .show_axes([false, true])
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .show(ui, |plot_ui| {
                plot_ui.line(
                    Line::new(rx_pts)
                        .name("Receive Kbps")
                        .color(palette.chart_net)
                        .fill(0.0_f32)
                        .width(2.0_f32),
                );
                plot_ui.line(
                    Line::new(tx_pts)
                        .name("Send Kbps")
                        .color(palette.chart_cpu)
                        .width(1.8_f32),
                );
            });

        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            self.draw_stat_block(
                ui,
                palette,
                "Receive",
                &format!(
                    "{:.1} Kbps",
                    (self.monitor.net_rx_bps as f32 * 8.0) / 1000.0
                ),
            );
            self.draw_stat_block(
                ui,
                palette,
                "Send",
                &format!(
                    "{:.1} Kbps",
                    (self.monitor.net_tx_bps as f32 * 8.0) / 1000.0
                ),
            );
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            for net in &self.monitor.network_list {
                Frame::none()
                    .fill(palette.bg_subtle)
                    .rounding(Rounding::same(6.0))
                    .stroke(Stroke::new(1.0_f32, palette.border_subtle))
                    .inner_margin(Margin::same(12.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("🌐  {}", net.name))
                                    .size(13.5)
                                    .strong(),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "Total Rx: {}   |   Total Tx: {}",
                                        format_bytes(net.total_rx_bytes),
                                        format_bytes(net.total_tx_bytes)
                                    ))
                                    .size(12.0)
                                    .color(palette.text_secondary),
                                );
                            });
                        });
                    });
                ui.add_space(8.0);
            }
        });
    }

    fn draw_stat_block(&self, ui: &mut egui::Ui, palette: &WinUiPalette, label: &str, value: &str) {
        ui.vertical(|ui| {
            ui.set_min_width(130.0);
            ui.label(
                RichText::new(label)
                    .size(11.5)
                    .color(palette.text_secondary),
            );
            ui.add_space(2.0);
            ui.label(
                RichText::new(value)
                    .size(18.0)
                    .strong()
                    .color(palette.text_primary),
            );
        });
        ui.add_space(18.0);
    }

    fn render_startup_page(&mut self, ui: &mut egui::Ui, palette: &WinUiPalette) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Startup apps")
                    .size(21.0)
                    .strong()
                    .color(palette.text_primary),
            );
            ui.label(
                RichText::new(format!("({} apps)", self.monitor.startup_apps.len()))
                    .size(12.5)
                    .color(palette.text_secondary),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("↻ Refresh Startup List").clicked() {
                    self.monitor.refresh_startup_apps();
                }
            });
        });
        ui.add_space(12.0);

        let query = self.search_query.trim().to_lowercase();
        let items: Vec<_> = self
            .monitor
            .startup_apps
            .iter()
            .filter(|item| {
                query.is_empty()
                    || item.name.to_lowercase().contains(&query)
                    || item.command.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();

        Frame::none()
            .fill(palette.bg_card)
            .rounding(Rounding::same(8.0))
            .stroke(Stroke::new(1.0_f32, palette.border_card))
            .inner_margin(Margin::symmetric(12.0, 10.0))
            .show(ui, |ui| {
                let avail_h = ui.available_height().max(200.0);
                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .min_scrolled_height(avail_h)
                    .max_scroll_height(avail_h)
                    .column(Column::initial(220.0).at_least(140.0).clip(true))
                    .column(Column::initial(100.0).at_least(85.0))
                    .column(Column::initial(115.0).at_least(90.0))
                    .column(Column::initial(200.0).at_least(140.0))
                    .column(Column::remainder().at_least(180.0).clip(true))
                    .header(34.0, |mut header| {
                        for title in ["Name", "Status", "Startup impact", "Location", "Command"] {
                            header.col(|ui| {
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(title)
                                        .size(12.5)
                                        .strong()
                                        .color(palette.text_secondary),
                                );
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(32.0, items.len(), |mut row| {
                            let entry = &items[row.index()];
                            row.col(|ui| {
                                ui.add_space(4.0);
                                ui.label(RichText::new(&entry.name).size(13.0).strong());
                            });
                            row.col(|ui| {
                                ui.add_space(4.0);
                                let (txt, col) = if entry.enabled {
                                    ("Enabled", palette.success)
                                } else {
                                    ("Disabled", palette.text_muted)
                                };
                                ui.label(RichText::new(txt).size(12.5).color(col));
                            });
                            row.col(|ui| {
                                ui.add_space(4.0);
                                let col = match entry.impact_estimate {
                                    "High" => palette.danger,
                                    "Medium" => palette.warning,
                                    _ => palette.text_secondary,
                                };
                                ui.label(
                                    RichText::new(entry.impact_estimate).size(12.5).color(col),
                                );
                            });
                            row.col(|ui| {
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(&entry.location)
                                        .size(12.0)
                                        .color(palette.text_secondary),
                                );
                            });
                            row.col(|ui| {
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(&entry.command)
                                        .monospace()
                                        .size(11.5)
                                        .color(palette.text_muted),
                                );
                            });
                        });
                    });
            });
    }

    fn render_users_page(&mut self, ui: &mut egui::Ui, palette: &WinUiPalette) {
        ui.label(
            RichText::new("Users")
                .size(21.0)
                .strong()
                .color(palette.text_primary),
        );
        ui.add_space(12.0);

        Frame::none()
            .fill(palette.bg_card)
            .rounding(Rounding::same(8.0))
            .stroke(Stroke::new(1.0_f32, palette.border_card))
            .inner_margin(Margin::same(18.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "👤  {} ({})",
                            self.monitor.current_user, self.monitor.host_name
                        ))
                        .size(16.0)
                        .strong(),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("Active Session")
                            .size(12.5)
                            .color(palette.success),
                    );
                });
                ui.add_space(14.0);
                ui.horizontal_wrapped(|ui| {
                    self.draw_stat_block(
                        ui,
                        palette,
                        "Active Processes",
                        &self.monitor.processes.len().to_string(),
                    );
                    self.draw_stat_block(
                        ui,
                        palette,
                        "CPU Usage",
                        &format!("{:.1}%", self.monitor.cpu_usage_total),
                    );
                    self.draw_stat_block(
                        ui,
                        palette,
                        "Memory Used",
                        &format_bytes(self.monitor.mem_used_bytes),
                    );
                    self.draw_stat_block(ui, palette, "OS", &self.monitor.os_name);
                });
            });
    }

    fn render_details_page(&mut self, ui: &mut egui::Ui, palette: &WinUiPalette) {
        let procs = self.filtered_and_sorted_flat_processes();
        ui.label(
            RichText::new("Details")
                .size(21.0)
                .strong()
                .color(palette.text_primary),
        );
        ui.add_space(12.0);

        Frame::none()
            .fill(palette.bg_card)
            .rounding(Rounding::same(8.0))
            .stroke(Stroke::new(1.0_f32, palette.border_card))
            .inner_margin(Margin::symmetric(12.0, 10.0))
            .show(ui, |ui| {
                let avail_h = ui.available_height().max(200.0);
                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .min_scrolled_height(avail_h)
                    .max_scroll_height(avail_h)
                    .column(Column::initial(200.0).at_least(120.0).clip(true))
                    .column(Column::initial(75.0).at_least(60.0))
                    .column(Column::initial(90.0).at_least(75.0))
                    .column(Column::initial(80.0).at_least(65.0))
                    .column(Column::initial(115.0).at_least(95.0))
                    .column(Column::initial(115.0).at_least(95.0))
                    .column(Column::remainder().at_least(180.0).clip(true))
                    .header(32.0, |mut header| {
                        for title in [
                            "Name",
                            "PID",
                            "Status",
                            "CPU",
                            "Working Set",
                            "Virtual Mem",
                            "Executable Path",
                        ] {
                            header.col(|ui| {
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(title)
                                        .size(12.0)
                                        .strong()
                                        .color(palette.text_secondary),
                                );
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(28.0, procs.len(), |mut row| {
                            let p = &procs[row.index()];
                            let is_sel = self.selected_pid == Some(p.pid);
                            row.set_selected(is_sel);

                            row.col(|ui| {
                                ui.add_space(4.0);
                                let resp = ui.add(
                                    egui::Label::new(RichText::new(&p.name).size(12.5))
                                        .sense(Sense::click()),
                                );
                                self.handle_row_interactions(ui, resp, p);
                            });
                            row.col(|ui| {
                                ui.add_space(4.0);
                                ui.label(RichText::new(p.pid.to_string()).monospace().size(12.0));
                            });
                            row.col(|ui| {
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(&p.status).size(12.0).color(palette.success),
                                );
                            });
                            row.col(|ui| {
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(format!("{:.1}%", p.cpu_pct))
                                        .monospace()
                                        .size(12.0),
                                );
                            });
                            row.col(|ui| {
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(format_bytes(p.memory_bytes))
                                        .monospace()
                                        .size(12.0),
                                );
                            });
                            row.col(|ui| {
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(format_bytes(p.virtual_mem_bytes))
                                        .monospace()
                                        .size(12.0)
                                        .color(palette.text_secondary),
                                );
                            });
                            row.col(|ui| {
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(&p.exe_path)
                                        .monospace()
                                        .size(11.5)
                                        .color(palette.text_muted),
                                );
                            });
                        });
                    });
            });
    }

    fn render_settings_page(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        palette: &WinUiPalette,
    ) {
        ui.label(
            RichText::new("Settings")
                .size(21.0)
                .strong()
                .color(palette.text_primary),
        );
        ui.add_space(14.0);

        Frame::none()
            .fill(palette.bg_card)
            .rounding(Rounding::same(8.0))
            .stroke(Stroke::new(1.0_f32, palette.border_card))
            .inner_margin(Margin::same(16.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("App theme").size(14.0).strong());
                        ui.label(
                            RichText::new("Select which WinUI 3 Fluent theme to display")
                                .size(12.0)
                                .color(palette.text_secondary),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .selectable_label(self.theme_mode == ThemeMode::Light, "☀ Light")
                            .clicked()
                        {
                            self.theme_mode = ThemeMode::Light;
                        }
                        if ui
                            .selectable_label(self.theme_mode == ThemeMode::Dark, "🌙 Dark")
                            .clicked()
                        {
                            self.theme_mode = ThemeMode::Dark;
                        }
                    });
                });
            });

        ui.add_space(10.0);

        Frame::none()
            .fill(palette.bg_card)
            .rounding(Rounding::same(8.0))
            .stroke(Stroke::new(1.0_f32, palette.border_card))
            .inner_margin(Margin::same(16.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Real-time update speed").size(14.0).strong());
                        ui.label(
                            RichText::new(
                                "Tip: Hold down the CTRL key at any time to temporarily freeze the process list",
                            )
                            .size(12.0)
                            .color(palette.text_secondary),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let ms = self.monitor.refresh_interval.as_millis();
                        if ui.selectable_label(ms == 2000, "Low (2.0s)").clicked() {
                            self.monitor.refresh_interval = Duration::from_millis(2000);
                        }
                        if ui.selectable_label(ms == 1000, "Normal (1.0s)").clicked() {
                            self.monitor.refresh_interval = Duration::from_millis(1000);
                        }
                        if ui.selectable_label(ms == 500, "High (0.5s)").clicked() {
                            self.monitor.refresh_interval = Duration::from_millis(500);
                        }
                    });
                });
            });

        ui.add_space(10.0);

        Frame::none()
            .fill(palette.bg_card)
            .rounding(Rounding::same(8.0))
            .stroke(Stroke::new(1.0_f32, palette.border_card))
            .inner_margin(Margin::same(16.0))
            .show(ui, |ui| {
                ui.checkbox(
                    &mut self.show_heatmap,
                    "Show Windows 11 resource heat-map shading on process columns",
                );
                ui.add_space(8.0);
                if ui
                    .checkbox(&mut self.always_on_top, "Always on top")
                    .changed()
                {
                    let level = if self.always_on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    };
                    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
                }
            });
    }

    fn render_this_pc_page(&mut self, ui: &mut egui::Ui, palette: &WinUiPalette) {
        let specs = self.monitor.pc_specs.clone();
        let disks = self.monitor.disk_list.clone();

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("This PC — System Specifications")
                    .size(21.0)
                    .strong()
                    .color(palette.text_primary),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("📋 Copy all specs").clicked() {
                    let gpu_summary = if specs.gpus.is_empty() {
                        "Standard Display Adapter".to_string()
                    } else {
                        specs
                            .gpus
                            .iter()
                            .map(|g| {
                                if g.vram_bytes > 0 {
                                    format!("{} ({})", g.name, format_bytes(g.vram_bytes))
                                } else {
                                    g.name.clone()
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(" / ")
                    };
                    let clip = format!(
                        "Device name: {}\nModel: {} ({})\nProcessor: {} ({} Cores / {} Logical)\nInstalled RAM: {}\nGPU: {}\nWindows: {} {} (Build {})",
                        specs.device_name,
                        specs.system_model,
                        specs.system_manufacturer,
                        specs.cpu_name,
                        specs.physical_cores,
                        specs.logical_cores,
                        format_bytes(specs.installed_ram_bytes),
                        gpu_summary,
                        specs.windows_edition,
                        specs.windows_version,
                        specs.windows_build
                    );
                    ui.output_mut(|o| o.copied_text = clip);
                    self.set_toast("Copied PC specifications to clipboard".to_string(), false);
                }
            });
        });

        ui.add_space(12.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // 1. Windows 11 System > About Top Hero Banner Card
                Frame::none()
                    .fill(palette.bg_card)
                    .rounding(Rounding::same(8.0))
                    .stroke(Stroke::new(1.0_f32, palette.border_card))
                    .inner_margin(Margin::same(18.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Stylized Laptop/PC emblem box
                            let (badge_rect, _) =
                                ui.allocate_exact_size(Vec2::new(62.0, 62.0), Sense::hover());
                            ui.painter().rect_filled(
                                badge_rect,
                                Rounding::same(12.0),
                                palette.accent_subtle_bg,
                            );
                            ui.painter().rect_stroke(
                                badge_rect,
                                Rounding::same(12.0),
                                Stroke::new(1.2_f32, palette.accent),
                            );
                            ui.painter().text(
                                badge_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "💻",
                                egui::FontId::proportional(30.0),
                                palette.accent,
                            );

                            ui.add_space(14.0);

                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(&specs.system_model)
                                        .size(19.5)
                                        .strong()
                                        .color(palette.text_primary),
                                );
                                ui.add_space(2.0);
                                ui.label(
                                    RichText::new(format!(
                                        "{}  ·  {}",
                                        specs.device_name, specs.system_manufacturer
                                    ))
                                    .size(13.0)
                                    .color(palette.text_secondary),
                                );
                                ui.add_space(2.0);
                                ui.label(
                                    RichText::new(format!(
                                        "{} (Version {}, Build {})",
                                        specs.windows_edition,
                                        specs.windows_version,
                                        specs.windows_build
                                    ))
                                    .size(12.0)
                                    .color(palette.accent),
                                );
                            });
                        });
                    });

                ui.add_space(14.0);

                // 2. Top 4 Hardware Pillars Grid (CPU, GPU, RAM, Storage)
                let total_storage_bytes: u64 = disks.iter().map(|d| d.total_bytes).sum();
                let used_storage_bytes: u64 = disks.iter().map(|d| d.used_bytes).sum();

                ui.columns(2, |cols| {
                    // Left Column: Processor (CPU) + Installed RAM
                    Frame::none()
                        .fill(palette.bg_card)
                        .rounding(Rounding::same(8.0))
                        .stroke(Stroke::new(1.0_f32, palette.border_card))
                        .inner_margin(Margin::same(16.0))
                        .show(&mut cols[0], |ui| {
                            ui.label(
                                RichText::new("🧠  Processor (CPU)")
                                    .size(12.5)
                                    .strong()
                                    .color(palette.chart_cpu),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(&specs.cpu_name)
                                    .size(15.0)
                                    .strong()
                                    .color(palette.text_primary),
                            );
                            ui.add_space(4.0);
                            let base_str = if specs.cpu_base_mhz > 0 {
                                format!("{:.2} GHz Base", specs.cpu_base_mhz as f32 / 1000.0)
                            } else {
                                format!("{:.2} GHz", self.monitor.cpu_freq_ghz)
                            };
                            ui.label(
                                RichText::new(format!(
                                    "{} Physical Cores  ·  {} Logical Processors  ·  {}",
                                    specs.physical_cores, specs.logical_cores, base_str
                                ))
                                .size(12.0)
                                .color(palette.text_secondary),
                            );
                        });

                    cols[0].add_space(10.0);

                    Frame::none()
                        .fill(palette.bg_card)
                        .rounding(Rounding::same(8.0))
                        .stroke(Stroke::new(1.0_f32, palette.border_card))
                        .inner_margin(Margin::same(16.0))
                        .show(&mut cols[0], |ui| {
                            ui.label(
                                RichText::new("⚡  Installed RAM")
                                    .size(12.5)
                                    .strong()
                                    .color(palette.chart_mem),
                            );
                            ui.add_space(4.0);
                            let approx_gb =
                                (specs.installed_ram_bytes as f64 / 1_073_741_824.0).ceil() as u64;
                            ui.label(
                                RichText::new(format!(
                                    "{} GB ({} usable)",
                                    approx_gb,
                                    format_bytes(specs.installed_ram_bytes)
                                ))
                                .size(15.0)
                                .strong()
                                .color(palette.text_primary),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!(
                                    "In use: {} ({:.0}%)  ·  Available: {}",
                                    format_bytes(self.monitor.mem_used_bytes),
                                    self.monitor.mem_usage_pct,
                                    format_bytes(self.monitor.mem_available_bytes)
                                ))
                                .size(12.0)
                                .color(palette.text_secondary),
                            );
                        });

                    // Right Column: Graphics (GPU) + Storage Summary
                    Frame::none()
                        .fill(palette.bg_card)
                        .rounding(Rounding::same(8.0))
                        .stroke(Stroke::new(1.0_f32, palette.border_card))
                        .inner_margin(Margin::same(16.0))
                        .show(&mut cols[1], |ui| {
                            ui.label(
                                RichText::new("🎮  Graphics Card (GPU)")
                                    .size(12.5)
                                    .strong()
                                    .color(palette.success),
                            );
                            ui.add_space(4.0);
                            if specs.gpus.is_empty() {
                                ui.label(
                                    RichText::new("Standard Windows Display Adapter")
                                        .size(15.0)
                                        .strong(),
                                );
                            } else {
                                for (idx, gpu) in specs.gpus.iter().enumerate() {
                                    let vram_str = if gpu.vram_bytes > 0 {
                                        format!(" ({})", format_bytes(gpu.vram_bytes))
                                    } else {
                                        String::new()
                                    };
                                    ui.label(
                                        RichText::new(format!(
                                            "GPU {}: {}{}",
                                            idx, gpu.name, vram_str
                                        ))
                                        .size(if idx == 0 { 14.5 } else { 13.0 })
                                        .strong()
                                        .color(palette.text_primary),
                                    );
                                }
                                if let Some(primary_gpu) = specs.gpus.first() {
                                    ui.add_space(2.0);
                                    ui.label(
                                        RichText::new(format!(
                                            "Driver: {} ({})",
                                            primary_gpu.driver_version, primary_gpu.provider
                                        ))
                                        .size(11.5)
                                        .color(palette.text_secondary),
                                    );
                                }
                            }
                        });

                    cols[1].add_space(10.0);

                    Frame::none()
                        .fill(palette.bg_card)
                        .rounding(Rounding::same(8.0))
                        .stroke(Stroke::new(1.0_f32, palette.border_card))
                        .inner_margin(Margin::same(16.0))
                        .show(&mut cols[1], |ui| {
                            ui.label(
                                RichText::new("🖴  Storage Hardware")
                                    .size(12.5)
                                    .strong()
                                    .color(palette.chart_disk),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!(
                                    "{} Total ({} used across {} partitions)",
                                    format_bytes(total_storage_bytes),
                                    format_bytes(used_storage_bytes),
                                    disks.len()
                                ))
                                .size(15.0)
                                .strong()
                                .color(palette.text_primary),
                            );
                            ui.add_space(4.0);
                            let hw_models = if specs.physical_disks.is_empty() {
                                "System Storage Drives".to_string()
                            } else {
                                specs.physical_disks.join("  ·  ")
                            };
                            ui.label(
                                RichText::new(format!("Drives: {}", hw_models))
                                    .size(12.0)
                                    .color(palette.text_secondary),
                            );
                        });
                });

                ui.add_space(16.0);

                // 3. Devices and Drives (Storage Partitions — Windows Explorer "This PC" style)
                ui.label(
                    RichText::new(format!("Devices and drives ({})", disks.len()))
                        .size(16.0)
                        .strong()
                        .color(palette.text_primary),
                );
                ui.add_space(8.0);

                for d in &disks {
                    Frame::none()
                        .fill(palette.bg_card)
                        .rounding(Rounding::same(8.0))
                        .stroke(Stroke::new(1.0_f32, palette.border_card))
                        .inner_margin(Margin::same(14.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let label = if d.name.is_empty() {
                                    format!("Local Disk ({})", d.mount_point)
                                } else {
                                    format!("{} ({})", d.name, d.mount_point)
                                };
                                ui.label(
                                    RichText::new(format!("🖴  {}", label))
                                        .size(14.5)
                                        .strong()
                                        .color(palette.text_primary),
                                );
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new(format!("{} · {}", d.file_system, d.kind))
                                        .size(12.0)
                                        .color(palette.text_muted),
                                );

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.small_button("📂 Open").clicked() {
                                        SystemMonitor::open_drive_in_explorer(&d.mount_point);
                                    }
                                    ui.add_space(8.0);
                                    ui.label(
                                        RichText::new(format!(
                                            "{} free of {} ({:.1}% used)",
                                            format_bytes(d.available_bytes),
                                            format_bytes(d.total_bytes),
                                            d.used_pct
                                        ))
                                        .size(12.5)
                                        .color(palette.text_secondary),
                                    );
                                });
                            });

                            ui.add_space(6.0);
                            let (bar_rect, _) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 10.0),
                                Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                bar_rect,
                                Rounding::same(5.0),
                                palette.bg_subtle,
                            );
                            let fill_w =
                                bar_rect.width() * (d.used_pct / 100.0).clamp(0.0, 1.0);
                            let bar_col = if d.used_pct >= 90.0 {
                                palette.danger
                            } else {
                                palette.accent
                            };
                            ui.painter().rect_filled(
                                Rect::from_min_size(
                                    bar_rect.min,
                                    Vec2::new(fill_w, bar_rect.height()),
                                ),
                                Rounding::same(5.0),
                                bar_col,
                            );
                        });
                    ui.add_space(8.0);
                }

                ui.add_space(8.0);

                // 4. Device Specifications Card (Windows 11 Settings -> System -> About style)
                ui.label(
                    RichText::new("Device specifications")
                        .size(16.0)
                        .strong()
                        .color(palette.text_primary),
                );
                ui.add_space(8.0);

                Frame::none()
                    .fill(palette.bg_card)
                    .rounding(Rounding::same(8.0))
                    .stroke(Stroke::new(1.0_f32, palette.border_card))
                    .inner_margin(Margin::same(16.0))
                    .show(ui, |ui| {
                        self.draw_spec_kv_row(ui, palette, "Device name", &specs.device_name);
                        ui.separator();
                        self.draw_spec_kv_row(
                            ui,
                            palette,
                            "Laptop / PC model",
                            &specs.system_model,
                        );
                        ui.separator();
                        self.draw_spec_kv_row(
                            ui,
                            palette,
                            "Manufacturer",
                            &specs.system_manufacturer,
                        );
                        if !specs.baseboard_model.is_empty() {
                            ui.separator();
                            self.draw_spec_kv_row(
                                ui,
                                palette,
                                "Motherboard / BIOS",
                                &format!("{} (BIOS {})", specs.baseboard_model, specs.bios_version),
                            );
                        }
                        ui.separator();
                        self.draw_spec_kv_row(
                            ui,
                            palette,
                            "Processor",
                            &format!(
                                "{} ({} Cores, {} Logical Processors)",
                                specs.cpu_name, specs.physical_cores, specs.logical_cores
                            ),
                        );
                        ui.separator();
                        self.draw_spec_kv_row(
                            ui,
                            palette,
                            "Installed RAM",
                            &format_bytes(specs.installed_ram_bytes),
                        );
                        for (idx, gpu) in specs.gpus.iter().enumerate() {
                            ui.separator();
                            let vram_info = if gpu.vram_bytes > 0 {
                                format!(
                                    "{} — {} VRAM (Driver {})",
                                    gpu.name,
                                    format_bytes(gpu.vram_bytes),
                                    gpu.driver_version
                                )
                            } else {
                                format!("{} (Driver {})", gpu.name, gpu.driver_version)
                            };
                            self.draw_spec_kv_row(
                                ui,
                                palette,
                                &format!("Graphics card {}", idx + 1),
                                &vram_info,
                            );
                        }
                        if !specs.physical_disks.is_empty() {
                            ui.separator();
                            self.draw_spec_kv_row(
                                ui,
                                palette,
                                "Physical disk drives",
                                &specs.physical_disks.join(", "),
                            );
                        }
                        ui.separator();
                        self.draw_spec_kv_row(ui, palette, "System type", &specs.system_type);
                    });

                ui.add_space(16.0);

                // 5. Windows Specifications Card
                ui.label(
                    RichText::new("Windows specifications")
                        .size(16.0)
                        .strong()
                        .color(palette.text_primary),
                );
                ui.add_space(8.0);

                Frame::none()
                    .fill(palette.bg_card)
                    .rounding(Rounding::same(8.0))
                    .stroke(Stroke::new(1.0_f32, palette.border_card))
                    .inner_margin(Margin::same(16.0))
                    .show(ui, |ui| {
                        self.draw_spec_kv_row(ui, palette, "Edition", &specs.windows_edition);
                        ui.separator();
                        self.draw_spec_kv_row(ui, palette, "Version", &specs.windows_version);
                        ui.separator();
                        self.draw_spec_kv_row(
                            ui,
                            palette,
                            "Installed on",
                            &specs.windows_install_date,
                        );
                        ui.separator();
                        self.draw_spec_kv_row(ui, palette, "OS build", &specs.windows_build);
                        if !specs.registered_owner.is_empty() {
                            ui.separator();
                            self.draw_spec_kv_row(
                                ui,
                                palette,
                                "Registered owner",
                                &specs.registered_owner,
                            );
                        }
                    });

                ui.add_space(16.0);
            });
    }

    fn draw_spec_kv_row(
        &self,
        ui: &mut egui::Ui,
        palette: &WinUiPalette,
        key: &str,
        value: &str,
    ) {
        ui.horizontal(|ui| {
            ui.set_min_height(26.0);
            ui.add_sized(
                [210.0, 22.0],
                egui::Label::new(
                    RichText::new(key)
                        .size(13.0)
                        .color(palette.text_secondary),
                ),
            );
            ui.label(
                RichText::new(value)
                    .size(13.0)
                    .strong()
                    .color(palette.text_primary),
            );
        });
    }

    fn render_modals_and_toasts(&mut self, ctx: &egui::Context, palette: &WinUiPalette) {
        if self.show_run_task_modal {
            let mut open = self.show_run_task_modal;
            egui::Window::new("Create new task")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(360.0);
                    ui.label(
                        RichText::new(
                            "Type the name of a program, folder, document, or Internet resource, and Windows will open it for you.",
                        )
                        .size(12.5)
                        .color(palette.text_secondary),
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.label("Open:");
                        let resp = ui.add_sized(
                            [290.0, 26.0],
                            egui::TextEdit::singleline(&mut self.run_task_input)
                                .hint_text("e.g. notepad, cmd, powershell"),
                        );
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            let cmd = self.run_task_input.clone();
                            match SystemMonitor::run_new_task(&cmd, self.run_task_as_admin) {
                                Ok(()) => {
                                    self.run_task_input.clear();
                                    self.show_run_task_modal = false;
                                    self.set_toast(format!("Started '{}'", cmd), false);
                                }
                                Err(e) => self.set_toast(e, true),
                            }
                        }
                    });
                    ui.add_space(8.0);
                    ui.checkbox(
                        &mut self.run_task_as_admin,
                        "🛡 Create this task with administrative privileges",
                    );
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            let cmd = self.run_task_input.clone();
                            match SystemMonitor::run_new_task(&cmd, self.run_task_as_admin) {
                                Ok(()) => {
                                    self.run_task_input.clear();
                                    self.show_run_task_modal = false;
                                    self.set_toast(format!("Started '{}'", cmd), false);
                                }
                                Err(e) => self.set_toast(e, true),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_run_task_modal = false;
                        }
                    });
                });
            if !open {
                self.show_run_task_modal = false;
            }
        }

        if let Some((msg, is_err, when)) = &self.status_toast {
            if when.elapsed() < Duration::from_secs(4) {
                egui::Area::new(Id::new("winui_toast"))
                    .anchor(egui::Align2::RIGHT_BOTTOM, [-18.0, -18.0])
                    .show(ctx, |ui| {
                        Frame::none()
                            .fill(palette.bg_card_selected)
                            .rounding(Rounding::same(6.0))
                            .stroke(Stroke::new(
                                1.2_f32,
                                if *is_err {
                                    palette.danger
                                } else {
                                    palette.accent
                                },
                            ))
                            .inner_margin(Margin::symmetric(14.0, 9.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(msg).size(12.5).color(if *is_err {
                                        palette.danger
                                    } else {
                                        palette.text_primary
                                    }),
                                );
                            });
                    });
            } else {
                self.status_toast = None;
            }
        }
    }
}

impl eframe::App for TaskManagerApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.mica_active {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            let p = WinUiPalette::for_mode(self.theme_mode, false);
            p.bg_base.to_normalized_gamma_f32()
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Hold CTRL to freeze the process list from moving, matching Windows Task Manager
        let ctrl_held = ctx.input(|i| i.modifiers.ctrl);
        self.monitor.tick(ctrl_held);

        if ctx.input(|i| i.key_pressed(Key::Delete)) {
            let shift_held = ctx.input(|i| i.modifiers.shift);
            self.trigger_kill_selected(shift_held);
        }

        let palette = apply_winui_style(ctx, self.theme_mode, self.mica_active);

        self.render_top_bar(ctx, &palette, ctrl_held);
        self.render_sidebar(ctx, &palette);

        egui::CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(palette.bg_base)
                    .inner_margin(Margin::same(18.0)),
            )
            .show(ctx, |ui| match self.nav_page {
                NavPage::Processes => self.render_processes_page(ui, &palette),
                NavPage::Performance => self.render_performance_split(ui, &palette),
                NavPage::ThisPc => self.render_this_pc_page(ui, &palette),
                NavPage::StartupApps => self.render_startup_page(ui, &palette),
                NavPage::Users => self.render_users_page(ui, &palette),
                NavPage::Details => self.render_details_page(ui, &palette),
                NavPage::Settings => self.render_settings_page(ctx, ui, &palette),
            });

        self.render_modals_and_toasts(ctx, &palette);

        ctx.request_repaint_after(Duration::from_millis(250));
    }
}
