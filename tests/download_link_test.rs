// Ticket 16a: the in-app link to the native download page lives in the pure
// menu model, so this test pins the table entry, the URL and the side effect
// without driving egui. The page itself is checked by scripts/check-site.sh.

use crate::support::menu_fixture::effect_of;

use vector_field_explorer::ui::menu::{
    self, ABOUT_SITE_URL, DOWNLOAD_URL, MenuAction, SideEffect,
};

#[test]
fn help_menu_links_to_the_native_download_page() {
    let help = menu::MENUS
        .iter()
        .find(|menu| menu.title == "Help")
        .expect("the Help menu exists");
    let item = help
        .items
        .iter()
        .find(|item| item.label == "Download desktop app")
        .expect("Help carries one link to the native download page");
    assert_eq!(item.action, MenuAction::OpenDownloadPage);

    let homes: Vec<(&str, &str)> = menu::MENUS
        .iter()
        .flat_map(|menu| {
            menu.items
                .iter()
                .filter(|item| item.action == MenuAction::OpenDownloadPage)
                .map(move |item| (menu.title, item.label))
        })
        .collect();
    assert_eq!(
        homes,
        vec![("Help", "Download desktop app")],
        "one download link, owned by the Help menu",
    );

    assert_eq!(
        DOWNLOAD_URL,
        "https://ttgg1.github.io/vector-field-explorer/download.html",
    );
    assert_eq!(
        DOWNLOAD_URL,
        format!("{ABOUT_SITE_URL}download.html"),
        "the download page sits beside the browser build on the project site",
    );
    assert_eq!(
        effect_of(MenuAction::OpenDownloadPage),
        SideEffect::OpenUrl(DOWNLOAD_URL.to_string()),
        "picking the item opens the download page in the browser",
    );
}
