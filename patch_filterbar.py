import re

with open('crates/ui-egui/src/panels/filterbar.rs', 'r') as f:
    content = f.read()

search_code = """    let mut chosen = None;
    egui::Popup::menu(&r).show(|ui| {
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for (label, patch, sel) in &items {
                if ui
                    .selectable_label(
                        *sel,
                        if matches!(id, "camera" | "lens" | "keyword") && patch.get(id).is_some_and(|value| !value.is_null()) {
                            label.as_str()
                        } else {
                            crate::i18n::tr(label)
                        },
                    )
                    .clicked()
                {
                    chosen = Some(patch.clone());
                }
            }
        });
    });"""

replace_code = """    let mut chosen = None;
    egui::Popup::menu(&r).show(|ui| {
        let is_searchable = matches!(id, "camera" | "lens" | "keyword");
        let mut search_query = String::new();
        if is_searchable {
            let search_id = ui.id().with("search");
            search_query = ui.data_mut(|d| d.get_temp::<String>(search_id).unwrap_or_default());

            let text_edit = egui::TextEdit::singleline(&mut search_query)
                .hint_text(crate::i18n::tr("Search…"));
            let response = ui.add(text_edit);

            if response.changed() {
                ui.data_mut(|d| d.insert_temp(search_id, search_query.clone()));
            }
            ui.separator();
        }
        let search_lower = search_query.to_lowercase();

        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for (label, patch, sel) in &items {
                let is_null_patch = patch.get(id).is_some_and(|value| value.is_null());
                if is_searchable && !is_null_patch && !search_lower.is_empty() && !label.to_lowercase().contains(&search_lower) {
                    continue;
                }
                if ui
                    .selectable_label(
                        *sel,
                        if is_searchable && !is_null_patch {
                            label.as_str()
                        } else {
                            crate::i18n::tr(label)
                        },
                    )
                    .clicked()
                {
                    chosen = Some(patch.clone());
                }
            }
        });
    });"""

if search_code in content:
    content = content.replace(search_code, replace_code)
    with open('crates/ui-egui/src/panels/filterbar.rs', 'w') as f:
        f.write(content)
    print("Patched successfully")
else:
    print("Could not find code to replace")
