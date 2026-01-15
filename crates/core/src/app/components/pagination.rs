use core::f32;
use std::{
    iter::{Skip, Take},
    slice::Iter,
};

use egui::Ui;

use crate::app::components::{self, ButtonSize};

pub struct Pagination {
    items_per_page: usize,
    current_page_index: usize,
    total_pages: usize,
    total_items: usize,
    ui_width: f32,
    // For internal use only.
    prepared_items: bool,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            items_per_page: 1,
            current_page_index: 0,
            total_pages: 1,
            total_items: 0,
            ui_width: 100.0,
            prepared_items: false,
        }
    }
}

impl Pagination {
    const ENSURE_PREPARED_ASSERT_MSG: &'static str =
        "Make sure to prepare the paginated items first";

    pub fn with_items_per_page(self, items_per_page: usize) -> Self {
        debug_assert!(items_per_page > 0, "items_per_page must be > 0");
        Self {
            items_per_page,
            ..self
        }
    }

    pub fn with_ui_width(self, ui_width: f32) -> Self {
        Self { ui_width, ..self }
    }

    pub fn width(&self) -> f32 {
        self.ui_width
    }

    pub fn pages(&self) -> usize {
        debug_assert!(self.prepared_items, "{}", Self::ENSURE_PREPARED_ASSERT_MSG);
        self.total_pages
    }

    pub fn items_per_page(&self) -> usize {
        debug_assert!(self.prepared_items, "{}", Self::ENSURE_PREPARED_ASSERT_MSG);
        self.items_per_page
    }

    pub fn current_page(&self) -> usize {
        debug_assert!(self.prepared_items, "{}", Self::ENSURE_PREPARED_ASSERT_MSG);
        self.current_page_index
    }

    pub fn prepare_current_page_items<'t, T>(&mut self, items: &'t [T]) -> Take<Skip<Iter<'t, T>>> {
        debug_assert!(
            self.items_per_page > 0,
            "items_per_page must be greater than zero"
        );
        let number_of_items_total = items.len();
        let total_pages = if number_of_items_total == 0 {
            0
        } else {
            (number_of_items_total + self.items_per_page - 1) / self.items_per_page
        };

        if total_pages == 0 {
            self.current_page_index = 0;
        } else if self.current_page_index >= total_pages {
            self.current_page_index = total_pages - 1;
        }

        let start_index =
            (self.current_page_index * self.items_per_page).min(number_of_items_total);
        let page_items = items.iter().skip(start_index).take(self.items_per_page);

        self.total_items = number_of_items_total;
        self.total_pages = total_pages;
        self.prepared_items = true;

        page_items
    }

    pub fn set_current_page(&mut self, page_index: usize) {
        self.current_page_index = page_index;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, page_ui: impl FnOnce(&mut Ui)) {
        debug_assert!(self.prepared_items, "{}", Self::ENSURE_PREPARED_ASSERT_MSG);

        ui.allocate_ui_with_layout(
            egui::vec2(self.ui_width, ui.available_height()), // fixed width, max height
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                ui.set_width(self.ui_width);
                // ui.set_width(ui.available_width());

                ui.vertical_centered(|ui| {
                    ui.set_width(self.ui_width);

                    let has_prev = self.current_page_index > 0;
                    let has_next =
                        self.total_pages > 0 && self.current_page_index + 1 < self.total_pages;

                    let scene_panel_page_button_sizes =
                        ButtonSize::Medium.with_width(ButtonSize::Medium.dim().0.x / 2.0);

                    ui.horizontal(|ui| {
                        ui.set_width(self.ui_width);

                        if components::button(ui, false, "◀", scene_panel_page_button_sizes)
                            && has_prev
                        {
                            self.current_page_index -= 1;
                        }

                        if components::button(ui, false, "▶", scene_panel_page_button_sizes)
                            && has_next
                        {
                            self.current_page_index += 1;
                        }
                    });

                    ui.add_space(5.0);

                    let (current_page_display, total_pages_display) = if self.total_pages == 0 {
                        (0, 0)
                    } else {
                        (self.current_page_index + 1, self.total_pages)
                    };

                    ui.label(format!(
                        "Page {} / {}",
                        current_page_display, total_pages_display
                    ));
                    ui.label(format!("Total: {}", self.total_items));
                });

                ui.add_space(5.0);

                page_ui(ui)
            },
        );
    }
}
