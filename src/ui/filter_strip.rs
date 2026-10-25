use eframe::egui::{self, Color32, RichText, Vec2};

use crate::config::{AppConfig, QuickSearchMode};

pub struct FilterStripAction {
    pub search_query_changed: bool,
    pub rescan_needed: bool,
}

pub fn render_filter_strip(
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    search_query: &mut String,
    filter_bar_active: &mut bool,
    is_dark: bool,
) -> FilterStripAction {
    let mut action = FilterStripAction {
        search_query_changed: false,
        rescan_needed: false,
    };

    ui.horizontal(|ui| {
        ui.label(RichText::new("Filters:").strong().small());

        // "All" reset pill
        let all_active = config.category_filters.show_images
            && config.category_filters.show_videos
            && config.category_filters.show_audio
            && config.category_filters.show_documents
            && config.category_filters.show_archives;

        if ui
            .selectable_label(all_active, RichText::new("All").small())
            .clicked()
        {
            config.category_filters.show_images = true;
            config.category_filters.show_videos = true;
            config.category_filters.show_audio = true;
            config.category_filters.show_documents = true;
            config.category_filters.show_archives = true;
            let _ = config.save();
        }

        // Category pills
        if ui
            .selectable_label(
                config.category_filters.show_images,
                RichText::new("Images").small(),
            )
            .clicked()
        {
            config.category_filters.show_images = !config.category_filters.show_images;
            let _ = config.save();
        }

        if ui
            .selectable_label(
                config.category_filters.show_videos,
                RichText::new("Videos").small(),
            )
            .clicked()
        {
            config.category_filters.show_videos = !config.category_filters.show_videos;
            let _ = config.save();
        }

        if ui
            .selectable_label(
                config.category_filters.show_audio,
                RichText::new("Audio").small(),
            )
            .clicked()
        {
            config.category_filters.show_audio = !config.category_filters.show_audio;
            let _ = config.save();
        }

        if ui
            .selectable_label(
                config.category_filters.show_documents,
                RichText::new("Documents").small(),
            )
            .clicked()
        {
            config.category_filters.show_documents = !config.category_filters.show_documents;
            let _ = config.save();
        }

        if ui
            .selectable_label(
                config.category_filters.show_archives,
                RichText::new("Archives").small(),
            )
            .clicked()
        {
            config.category_filters.show_archives = !config.category_filters.show_archives;
            let _ = config.save();
        }

        ui.separator();

        // Duplicates Toggle Pill
        let dup_pill = ui.selectable_label(
            config.show_duplicates_only,
            RichText::new("Duplicates Only")
                .small()
                .color(if config.show_duplicates_only {
                    Color32::from_rgb(255, 180, 50)
                } else if is_dark {
                    Color32::LIGHT_GRAY
                } else {
                    Color32::DARK_GRAY
                }),
        );
        if dup_pill.clicked() {
            config.show_duplicates_only = !config.show_duplicates_only;
            let _ = config.save();
            action.rescan_needed = true;
        }

        // Flatten Toggle Pill
        let flat_pill = ui.selectable_label(
            config.flatten_subfolders,
            RichText::new("Flatten (F6)").small(),
        );
        if flat_pill.clicked() {
            config.flatten_subfolders = !config.flatten_subfolders;
            let _ = config.save();
            action.rescan_needed = true;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Quick search mode indicator
            let mode_label = match config.quick_search_mode {
                QuickSearchMode::JumpToPrefix => "[Jump (F2)]",
                QuickSearchMode::LiveFilter => "[Filter (F3)]",
                QuickSearchMode::Disabled => "[Search Off]",
            };
            if ui.small_button(mode_label).on_hover_text("Click or press F2 to cycle search mode").clicked() {
                config.quick_search_mode = match config.quick_search_mode {
                    QuickSearchMode::JumpToPrefix => QuickSearchMode::LiveFilter,
                    QuickSearchMode::LiveFilter => QuickSearchMode::Disabled,
                    QuickSearchMode::Disabled => QuickSearchMode::JumpToPrefix,
                };
                let _ = config.save();
            }

            // Search text box
            let search_width = 160.0;
            let edit = ui.add_sized(
                Vec2::new(search_width, 20.0),
                egui::TextEdit::singleline(search_query)
                    .hint_text("Filter by name, .ext..."),
            );

            if edit.changed() {
                action.search_query_changed = true;
                *filter_bar_active = !search_query.is_empty();
            }
        });
    });

    action
}
