use crate::quad::TripleLayerQuadAllocator;
use crate::termwindow::render::{forces_opaque_kaku_tui_window_background, RenderScreenLineParams};
use crate::termwindow::PaneInformation;
use crate::utilsprites::RenderMetrics;
use config::{ConfigHandle, TabBarColors};
use mux::renderable::RenderableDimensions;
use termwiz::cell::unicode_column_width;
use termwiz::surface::SEQ_ZERO;
use wezterm_term::color::ColorAttribute;
use window::color::LinearRgba;

impl crate::TermWindow {
    pub fn paint_tab_bar(&mut self, layers: &mut TripleLayerQuadAllocator) -> anyhow::Result<()> {
        let border = self.get_os_border();
        let tab_bar_height = self.tab_bar_pixel_height()?;
        let tab_bar_y = if self.config.tab_bar_at_bottom {
            ((self.dimensions.pixel_height as f32) - tab_bar_height - border.bottom.get() as f32)
                .max(0.)
        } else {
            // Offset below the OS top inset so cells aren't clipped by the
            // macOS rounded corner / integrated buttons window mask.
            border.top.get() as f32
        };
        let panes = self.get_panes_to_render();
        let force_opaque_tab_bar_background = forces_opaque_kaku_tui_window_background(&panes);

        if self.config.use_fancy_tab_bar {
            if self.fancy_tab_bar.is_none() {
                let palette = self.palette().clone();
                let tab_bar = self.build_fancy_tab_bar(&palette)?;
                self.fancy_tab_bar.replace(tab_bar);
            }

            // In transparent mode, fill the tab bar area with a transparent
            // background so it blends consistently with the window.
            let window_is_transparent =
                !self.window_background.is_empty() || self.config.window_background_opacity != 1.0;
            if window_is_transparent && !force_opaque_tab_bar_background {
                let tab_bar_bg = if let Some(active) = self.get_active_pane_or_overlay() {
                    active
                        .palette()
                        .background
                        .to_linear()
                        .mul_alpha(self.config.window_background_opacity)
                } else {
                    self.palette()
                        .background
                        .to_linear()
                        .mul_alpha(self.config.window_background_opacity)
                };
                self.filled_rectangle(
                    layers,
                    0,
                    euclid::rect(
                        0.0,
                        tab_bar_y,
                        self.dimensions.pixel_width as f32,
                        tab_bar_height,
                    ),
                    tab_bar_bg,
                )?;
            }

            let mut fancy_ui_items = self.paint_fancy_tab_bar()?;
            self.ui_items.append(&mut fancy_ui_items);
            return Ok(());
        }

        let palette = self.palette().clone();

        let tab_metrics = if self.config.tab_bar_at_bottom {
            // Bottom tabs have no rounded titlebar mask above them, so keep the
            // compact natural height used by earlier releases.
            RenderMetrics::with_font_metrics(&self.fonts.default_font()?.metrics())
        } else {
            // Top tabs sit under the macOS titlebar mask; honor line_height so
            // tall fonts don't clip against the top edge.
            self.render_metrics
        };

        self.ui_items.append(&mut self.tab_bar.compute_ui_items(
            tab_bar_y as usize,
            tab_metrics.cell_size.height as usize,
            tab_metrics.cell_size.width as usize,
        ));

        let window_is_transparent =
            !self.window_background.is_empty() || self.config.window_background_opacity != 1.0;
        let effective_window_is_transparent =
            window_is_transparent && !force_opaque_tab_bar_background;
        let gl_state = self.render_state.as_ref().unwrap();
        let white_space = gl_state.util_sprites.white_space.texture_coords();
        let filled_box = gl_state.util_sprites.filled_box.texture_coords();
        let default_bg = palette
            .resolve_bg(ColorAttribute::Default)
            .to_linear()
            .mul_alpha(if effective_window_is_transparent {
                0.
            } else {
                self.config.text_background_opacity
            });

        if effective_window_is_transparent {
            let tab_bar_bg = if let Some(active) = self.get_active_pane_or_overlay() {
                active
                    .palette()
                    .background
                    .to_linear()
                    .mul_alpha(self.config.window_background_opacity)
            } else {
                palette
                    .background
                    .to_linear()
                    .mul_alpha(self.config.window_background_opacity)
            };
            self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    0.0,
                    tab_bar_y,
                    self.dimensions.pixel_width as f32,
                    tab_bar_height,
                ),
                tab_bar_bg,
            )?;
        }

        self.render_screen_line(
            RenderScreenLineParams {
                top_pixel_y: tab_bar_y,
                left_pixel_x: 0.,
                pixel_width: self.dimensions.pixel_width as f32,
                stable_line_idx: None,
                line: self.tab_bar.line(),
                selection: 0..0,
                cursor: &Default::default(),
                palette: &palette,
                dims: &RenderableDimensions {
                    cols: self.dimensions.pixel_width / tab_metrics.cell_size.width as usize,
                    physical_top: 0,
                    scrollback_rows: 0,
                    scrollback_top: 0,
                    viewport_rows: 1,
                    dpi: self.terminal_size.dpi,
                    pixel_height: tab_metrics.cell_size.height as usize,
                    pixel_width: self.terminal_size.pixel_width,
                    reverse_video: false,
                },
                config: &self.config,
                cursor_border_color: LinearRgba::default(),
                foreground: palette.foreground.to_linear(),
                pane: None,
                is_active: true,
                selection_fg: LinearRgba::default(),
                selection_bg: LinearRgba::default(),
                cursor_fg: LinearRgba::default(),
                cursor_bg: LinearRgba::default(),
                cursor_is_default_color: true,
                white_space,
                filled_box,
                window_is_transparent: effective_window_is_transparent,
                default_bg,
                style: None,
                font: None,
                use_pixel_positioning: self.config.experimental_pixel_positioning,
                render_metrics: tab_metrics,
                shape_key: None,
                password_input: false,
            },
            layers,
        )?;

        // --- Multi-pane hover popup: render pane list above tab bar ---
        if self.show_tab_popup {
            let tab_ui_item = self.ui_items.iter().find(|ui| {
                matches!(
                    ui.item_type,
                    crate::termwindow::UIItemType::TabBar(
                        crate::tabbar::TabBarItem::Tab { tab_idx, .. },
                    ) if self.popup_tab_idx == Some(tab_idx)
                )
            });

            if let Some(tab_ui) = tab_ui_item {
                let cell_h = tab_metrics.cell_size.height as f32;
                let popup_width_px = tab_ui.width as f32;
                let popup_width_cells = tab_ui.width / tab_metrics.cell_size.width as usize;
                let tab_left_px = tab_ui.x as f32;

                // Get TabInformation for the hovered tab
                let tab_info = self.popup_tab_idx.and_then(|idx| {
                    self.get_tab_information()
                        .into_iter()
                        .find(|t| t.tab_index == idx)
                });

                if let Some(ref tab_info) = tab_info {
                    let non_active: Vec<&PaneInformation> =
                        tab_info.panes.iter().filter(|p| !p.is_active).collect();
                    let num_rows = non_active.len();
                    let popup_px = num_rows as f32 * cell_h;
                    let popup_top = 0f32.max(tab_bar_y - popup_px);

                    let tab_bar_colors = self
                        .config
                        .resolved_palette
                        .tab_bar
                        .clone()
                        .unwrap_or_else(TabBarColors::default);

                    let popup_bg = tab_bar_colors.inactive_tab().bg_color.to_linear();
                    self.filled_rectangle(
                        layers,
                        1,
                        euclid::rect(tab_left_px, popup_top, popup_width_px, popup_px),
                        popup_bg,
                    )?;

                    for (row, pane) in non_active.iter().enumerate() {
                        let hovered = self.popup_hover_row == Some(row);

                        // Hovered row gets its background on layer 1 (above pane text)
                        if hovered {
                            let hover_bg =
                                tab_bar_colors.inactive_tab_hover().bg_color.to_linear();
                            self.filled_rectangle(
                                layers,
                                1,
                                euclid::rect(
                                    tab_left_px,
                                    popup_top + row as f32 * cell_h,
                                    popup_width_px,
                                    cell_h,
                                ),
                                hover_bg,
                            )?;
                        }

                        let attrs = if hovered {
                            tab_bar_colors.inactive_tab_hover().as_cell_attributes()
                        } else {
                            tab_bar_colors.inactive_tab().as_cell_attributes()
                        };

                        // Get title from tab_display_title (Lua), same as tab bar
                        let title = crate::tabbar::call_pane_display_title(pane, &self.config)
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| pane.title.clone());

                        let row_y = popup_top + row as f32 * cell_h;

                        let mut popup_line = wezterm_term::Line::with_width(0, SEQ_ZERO);
                        for fill in 0..popup_width_cells {
                            popup_line.insert_cell(
                                fill,
                                wezterm_term::Cell::blank_with_attrs(attrs.clone()),
                                popup_width_cells,
                                SEQ_ZERO,
                            );
                        }
                        // Left-align title text with 1-cell padding
                        // Show "…" ellipsis if title overflows popup width
                        let mut col = 1;
                        let mut truncated = false;
                        for c in title.chars() {
                            let cw = unicode_column_width(&c.to_string(), None);
                            if col + cw > popup_width_cells {
                                truncated = true;
                                break;
                            }
                            popup_line.insert_cell(
                                col,
                                wezterm_term::Cell::new_grapheme(&c.to_string(), attrs.clone(), None),
                                popup_width_cells,
                                SEQ_ZERO,
                            );
                            col += cw;
                        }
                        if truncated && col > 1 {
                            popup_line.insert_cell(
                                col.saturating_sub(1),
                                wezterm_term::Cell::new_grapheme("\u{2026}", attrs.clone(), None),
                                popup_width_cells,
                                SEQ_ZERO,
                            );
                        }

                        self.render_screen_line(
                            RenderScreenLineParams {
                                top_pixel_y: row_y,
                                left_pixel_x: tab_left_px,
                                pixel_width: popup_width_px,
                                stable_line_idx: None,
                                line: &popup_line,
                                selection: 0..0,
                                cursor: &Default::default(),
                                palette: &palette,
                                dims: &RenderableDimensions {
                                    cols: popup_width_cells,
                                    physical_top: 0,
                                    scrollback_rows: 0,
                                    scrollback_top: 0,
                                    viewport_rows: 1,
                                    dpi: self.terminal_size.dpi,
                                    pixel_height: tab_metrics.cell_size.height as usize,
                                    pixel_width: popup_width_px as usize,
                                    reverse_video: false,
                                },
                                config: &self.config,
                                cursor_border_color: LinearRgba::default(),
                                foreground: palette.foreground.to_linear(),
                                pane: None,
                                is_active: true,
                                selection_fg: LinearRgba::default(),
                                selection_bg: LinearRgba::default(),
                                cursor_fg: LinearRgba::default(),
                                cursor_bg: LinearRgba::default(),
                                cursor_is_default_color: true,
                                white_space,
                                filled_box,
                                window_is_transparent: false,
                                default_bg: LinearRgba::default(),
                                style: None,
                                font: None,
                                use_pixel_positioning: false,
                                render_metrics: tab_metrics,
                                shape_key: None,
                            password_input: false,
                        },
                        layers,
                    )?;
                    }
                }
            }
        }

        Ok(())
    }

    pub fn tab_bar_pixel_height_impl(
        config: &ConfigHandle,
        fontconfig: &wezterm_font::FontConfiguration,
        render_metrics: &RenderMetrics,
    ) -> anyhow::Result<f32> {
        if config.use_fancy_tab_bar {
            let font = fontconfig.title_font()?;
            Ok((font.metrics().cell_height.get() as f32 * 1.75).ceil())
        } else if config.tab_bar_at_bottom {
            Ok(render_metrics.natural_cell_height as f32)
        } else {
            Ok(render_metrics.cell_size.height as f32)
        }
    }

    /// Cheap approximation of tab bar height that avoids the ~485ms cost of
    /// resolving the title font on macOS cold start (CoreText substitution
    /// lookup + HarfBuzz shaper init). Used only to compute initial window
    /// dimensions; the real height is computed lazily on first render via
    /// `tab_bar_pixel_height()`.
    pub fn estimated_tab_bar_pixel_height(
        config: &ConfigHandle,
        render_metrics: &RenderMetrics,
    ) -> f32 {
        if config.use_fancy_tab_bar {
            // Mirror tab_bar_pixel_height_impl's fancy-path formula, but use
            // the terminal cell height as a stand-in for the title font cell
            // height. The two differ by ~1-2 pixels in typical configs.
            (render_metrics.cell_size.height as f32 * 1.75).ceil()
        } else if config.tab_bar_at_bottom {
            render_metrics.natural_cell_height as f32
        } else {
            render_metrics.cell_size.height as f32
        }
    }

    pub fn tab_bar_pixel_height(&self) -> anyhow::Result<f32> {
        Self::tab_bar_pixel_height_impl(&self.config, &self.fonts, &self.render_metrics)
    }
}
