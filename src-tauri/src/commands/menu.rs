use super::CmdResult;
use serde::{Deserialize, Serialize};
use tauri::{
    menu::{
        AboutMetadata, CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID,
        WINDOW_SUBMENU_ID,
    },
    AppHandle, Emitter, Manager, Runtime,
};

const MENU_EVENT: &str = "menu://action";
const SETTINGS_ID: &str = "omb:settings";
const DEVTOOLS_ID: &str = "omb:toggle-devtools";
const SPACE_SETTINGS_ID: &str = "omb:space-settings";
const SPACE_PREFIX: &str = "omb:space:";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuSpace {
    id: String,
    name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MenuActionPayload {
    action: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    space_id: Option<String>,
}

#[tauri::command]
pub fn set_spaces_menu<R: Runtime>(
    app: AppHandle<R>,
    spaces: Vec<MenuSpace>,
    active_id: Option<String>,
) -> CmdResult<()> {
    set_app_menu(&app, &spaces, active_id.as_deref()).map_err(|e| e.to_string())
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    set_app_menu(app, &[], None)?;
    app.on_menu_event(|app, event| match event.id().as_ref() {
        SETTINGS_ID => emit_menu_action(app, "open-settings", None),
        DEVTOOLS_ID => toggle_devtools(app),
        SPACE_SETTINGS_ID => emit_menu_action(app, "space-settings", None),
        id => {
            if let Some(space_id) = parse_space_menu_id(id) {
                // Native check items toggle themselves on click; keep the clicked space checked.
                // The frontend rebuilds the menu (and clears other checks) once the switch lands.
                if let Some(item) = app.menu().and_then(|m| find_check_item(&m, id)) {
                    let _ = item.set_checked(true);
                }
                emit_menu_action(app, "switch-space", Some(space_id.to_string()));
            }
        }
    });
    Ok(())
}

fn set_app_menu<R: Runtime>(
    app: &AppHandle<R>,
    spaces: &[MenuSpace],
    active_id: Option<&str>,
) -> tauri::Result<()> {
    app.set_menu(build_menu(app, spaces, active_id)?)?;
    Ok(())
}

fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    spaces: &[MenuSpace],
    active_id: Option<&str>,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;

    #[cfg(target_os = "macos")]
    menu.append(&app_submenu(app)?)?;
    #[cfg(not(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    )))]
    menu.append(&file_submenu(app)?)?;

    menu.append(&edit_submenu(app)?)?;
    menu.append(&view_submenu(app)?)?;
    menu.append(&spaces_submenu(app, spaces, active_id)?)?;
    menu.append(&window_submenu(app)?)?;
    menu.append(&help_submenu(app)?)?;
    Ok(menu)
}

#[cfg(target_os = "macos")]
fn app_submenu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let about_metadata = about_metadata(app);
    Submenu::with_items(
        app,
        app.package_info().name.clone(),
        true,
        &[
            &PredefinedMenuItem::about(app, None, Some(about_metadata))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, SETTINGS_ID, "Settings…", true, Some("Cmd+,"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
)))]
fn file_submenu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    #[cfg(target_os = "macos")]
    {
        Submenu::with_items(
            app,
            "File",
            true,
            &[&PredefinedMenuItem::close_window(app, None)?],
        )
    }
    #[cfg(not(target_os = "macos"))]
    Submenu::with_items(
        app,
        "File",
        true,
        &[
            &MenuItem::with_id(app, SETTINGS_ID, "Settings…", true, Some("Ctrl+,"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )
}

fn edit_submenu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )
}

fn view_submenu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    #[cfg(target_os = "macos")]
    {
        Submenu::with_items(
            app,
            "View",
            true,
            &[
                &PredefinedMenuItem::fullscreen(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(
                    app,
                    DEVTOOLS_ID,
                    "Toggle Developer Tools",
                    true,
                    Some("Cmd+Alt+I"),
                )?,
            ],
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        #[cfg(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        {
            return Submenu::with_items(
                app,
                "View",
                true,
                &[
                    &MenuItem::with_id(app, SETTINGS_ID, "Settings…", true, Some("Ctrl+,"))?,
                    &PredefinedMenuItem::separator(app)?,
                    &MenuItem::with_id(
                        app,
                        DEVTOOLS_ID,
                        "Toggle Developer Tools",
                        true,
                        Some("Ctrl+Shift+I"),
                    )?,
                ],
            );
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        )))]
        Submenu::with_items(
            app,
            "View",
            true,
            &[&MenuItem::with_id(
                app,
                DEVTOOLS_ID,
                "Toggle Developer Tools",
                true,
                Some("Ctrl+Shift+I"),
            )?],
        )
    }
}

fn spaces_submenu<R: Runtime>(
    app: &AppHandle<R>,
    spaces: &[MenuSpace],
    active_id: Option<&str>,
) -> tauri::Result<Submenu<R>> {
    let submenu = Submenu::with_id(app, "omb:spaces", "Spaces", true)?;
    if spaces.is_empty() {
        submenu.append(&MenuItem::with_id(
            app,
            "omb:no-spaces",
            "No Spaces",
            false,
            None::<&str>,
        )?)?;
    } else {
        for (index, space) in spaces.iter().enumerate() {
            let accelerator = (index < 9).then(|| format!("CmdOrCtrl+{}", index + 1));
            submenu.append(&CheckMenuItem::with_id(
                app,
                format!("{SPACE_PREFIX}{}", space.id),
                menu_label(&space.name),
                true,
                Some(space.id.as_str()) == active_id,
                accelerator.as_deref(),
            )?)?;
        }
    }
    submenu.append(&PredefinedMenuItem::separator(app)?)?;
    submenu.append(&MenuItem::with_id(
        app,
        SPACE_SETTINGS_ID,
        "Space Settings…",
        active_id.is_some(),
        None::<&str>,
    )?)?;
    Ok(submenu)
}

fn window_submenu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    Submenu::with_id_and_items(
        app,
        WINDOW_SUBMENU_ID,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )
}

fn help_submenu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    Submenu::with_id_and_items(
        app,
        HELP_SUBMENU_ID,
        "Help",
        true,
        &[
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::about(app, None, Some(about_metadata(app)))?,
        ],
    )
}

fn about_metadata<R: Runtime>(app: &AppHandle<R>) -> AboutMetadata<'_> {
    let pkg_info = app.package_info();
    let config = app.config();
    AboutMetadata {
        name: Some(pkg_info.name.clone()),
        version: Some(pkg_info.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    }
}

fn find_check_item<R: Runtime>(menu: &Menu<R>, id: &str) -> Option<CheckMenuItem<R>> {
    menu.get("omb:spaces")?
        .as_submenu()?
        .get(id)?
        .as_check_menuitem()
        .cloned()
}

fn emit_menu_action<R: Runtime>(
    app: &AppHandle<R>,
    action: &'static str,
    space_id: Option<String>,
) {
    let _ = app.emit(MENU_EVENT, MenuActionPayload { action, space_id });
}

fn toggle_devtools<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_devtools_open() {
            window.close_devtools();
        } else {
            window.open_devtools();
        }
    }
}

pub fn parse_space_menu_id(id: &str) -> Option<&str> {
    id.strip_prefix(SPACE_PREFIX)
        .filter(|space_id| !space_id.is_empty())
}

fn menu_label(label: &str) -> String {
    label.replace('&', "&&")
}

#[cfg(test)]
mod tests {
    use super::{menu_label, parse_space_menu_id};

    #[test]
    fn parses_space_menu_ids() {
        assert_eq!(parse_space_menu_id("omb:space:travel"), Some("travel"));
        assert_eq!(
            parse_space_menu_id("omb:space:space:with:colon"),
            Some("space:with:colon")
        );
        assert_eq!(parse_space_menu_id("omb:space:"), None);
        assert_eq!(parse_space_menu_id("omb:settings"), None);
    }

    #[test]
    fn escapes_ampersands_for_menu_labels() {
        assert_eq!(menu_label("R&D && Archive"), "R&&D &&&& Archive");
    }
}
