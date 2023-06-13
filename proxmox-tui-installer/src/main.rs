#![forbid(unsafe_code)]

mod options;
mod system;
mod utils;
mod views;

use crate::options::*;
use cursive::{
    event::Event,
    view::{Nameable, Resizable, ViewWrapper},
    views::{
        Button, Checkbox, Dialog, DummyView, EditView, LinearLayout, PaddedView, Panel,
        ResizedView, ScrollView, SelectView, TextView,
    },
    Cursive, View,
};
use std::net::IpAddr;
use views::{BootdiskOptionsView, CidrAddressEditView, FormView, TableView, TableViewItem};

// TextView::center() seems to garble the first two lines, so fix it manually here.
const LOGO: &str = r#"
       ____                                          _    __ _____
      / __ \_________  _  ______ ___  ____  _  __   | |  / / ____/
  / /_/ / ___/ __ \| |/_/ __ `__ \/ __ \| |/_/   | | / / __/
 / ____/ /  / /_/ />  </ / / / / / /_/ />  <     | |/ / /___
/_/   /_/   \____/_/|_/_/ /_/ /_/\____/_/|_|     |___/_____/
"#;

const TITLE: &str = "Proxmox VE Installer";

struct InstallerView {
    view: ResizedView<LinearLayout>,
}

impl InstallerView {
    pub fn new<T: View>(view: T, next_cb: Box<dyn Fn(&mut Cursive)>) -> Self {
        let inner = LinearLayout::vertical()
            .child(PaddedView::lrtb(0, 0, 1, 1, view))
            .child(PaddedView::lrtb(
                1,
                1,
                0,
                0,
                LinearLayout::horizontal()
                    .child(abort_install_button())
                    .child(DummyView.full_width())
                    .child(Button::new("Previous", switch_to_prev_screen))
                    .child(DummyView)
                    .child(Button::new("Next", next_cb)),
            ));

        Self::with_raw(inner)
    }

    pub fn with_raw<T: View>(view: T) -> Self {
        let inner = LinearLayout::vertical()
            .child(PaddedView::lrtb(1, 1, 0, 1, TextView::new(LOGO).center()))
            .child(Dialog::around(view).title(TITLE));

        Self {
            // Limit the maximum to something reasonable, such that it won't get spread out much
            // depending on the screen.
            view: ResizedView::with_max_size((120, 40), inner),
        }
    }
}

impl ViewWrapper for InstallerView {
    cursive::wrap_impl!(self.view: ResizedView<LinearLayout>);
}

#[derive(Clone)]
struct InstallerData {
    options: InstallerOptions,
    available_disks: Vec<Disk>,
}

fn main() {
    let mut siv = cursive::termion();

    if let Err(err) = system::has_min_requirements() {
        siv.add_layer(Dialog::around(TextView::new(err)).button("Ok", Cursive::quit));
        siv.run();
        return;
    }

    siv.clear_global_callbacks(Event::CtrlChar('c'));
    siv.set_on_pre_event(Event::CtrlChar('c'), trigger_abort_install_dialog);

    // TODO: retrieve actual disk info
    let available_disks = vec![Disk {
        path: "/dev/vda".to_owned(),
        size: 17179869184,
    }];

    siv.set_user_data(InstallerData {
        options: InstallerOptions {
            bootdisk: BootdiskOptions::defaults_from(&available_disks[0]),
            timezone: TimezoneOptions::default(),
            password: PasswordOptions::default(),
            network: NetworkOptions::default(),
        },
        available_disks,
    });

    add_next_screen(&mut siv, &license_dialog);
    siv.run();
}

fn add_next_screen(siv: &mut Cursive, constructor: &dyn Fn(&mut Cursive) -> InstallerView) {
    let v = constructor(siv);
    siv.add_active_screen();
    siv.screen_mut().add_layer(v);
}

fn switch_to_prev_screen(siv: &mut Cursive) {
    let id = siv.active_screen().saturating_sub(1);
    siv.set_screen(id);
}

#[cfg(not(debug_assertions))]
fn yes_no_dialog(
    siv: &mut Cursive,
    title: &str,
    text: &str,
    callback: &'static dyn Fn(&mut Cursive),
) {
    siv.add_layer(
        Dialog::around(TextView::new(text))
            .title(title)
            .dismiss_button("No")
            .button("Yes", callback),
    )
}

fn trigger_abort_install_dialog(siv: &mut Cursive) {
    #[cfg(debug_assertions)]
    siv.quit();

    #[cfg(not(debug_assertions))]
    yes_no_dialog(
        siv,
        "Abort installation?",
        "Are you sure you want to abort the installation?",
        &Cursive::quit,
    )
}

fn abort_install_button() -> Button {
    Button::new("Abort", trigger_abort_install_dialog)
}

fn get_eula() -> String {
    // TODO: properly using info from Proxmox::Install::Env::setup()
    std::fs::read_to_string("/cdrom/EULA")
        .unwrap_or_else(|_| "< Debug build - ignoring non-existing EULA >".to_owned())
}

fn license_dialog(_: &mut Cursive) -> InstallerView {
    let inner = LinearLayout::vertical()
        .child(PaddedView::lrtb(
            0,
            0,
            1,
            0,
            TextView::new("END USER LICENSE AGREEMENT (EULA)").center(),
        ))
        .child(Panel::new(ScrollView::new(
            TextView::new(get_eula()).center(),
        )))
        .child(PaddedView::lrtb(
            1,
            1,
            1,
            0,
            LinearLayout::horizontal()
                .child(abort_install_button())
                .child(DummyView.full_width())
                .child(Button::new("I agree", |siv| {
                    add_next_screen(siv, &bootdisk_dialog)
                })),
        ));

    InstallerView::with_raw(inner)
}

fn bootdisk_dialog(siv: &mut Cursive) -> InstallerView {
    let data = siv.user_data::<InstallerData>().cloned().unwrap();

    InstallerView::new(
        BootdiskOptionsView::new(&data.available_disks, &data.options.bootdisk)
            .with_name("bootdisk-options"),
        Box::new(|siv| {
            let options = siv
                .call_on_name("bootdisk-options", BootdiskOptionsView::get_values)
                .flatten();

            if let Some(options) = options {
                siv.with_user_data(|data: &mut InstallerData| {
                    data.options.bootdisk = options;
                });

                add_next_screen(siv, &timezone_dialog);
            } else {
                siv.add_layer(Dialog::info("Invalid values"));
            }
        }),
    )
}

fn timezone_dialog(siv: &mut Cursive) -> InstallerView {
    let options = siv
        .user_data::<InstallerData>()
        .map(|data| data.options.timezone.clone())
        .unwrap_or_default();

    let inner = FormView::new()
        .child("Country", EditView::new().content("Austria"))
        .child("Timezone", EditView::new().content(options.timezone))
        .child(
            "Keyboard layout",
            EditView::new().content(options.kb_layout),
        )
        .with_name("timezone-options");

    InstallerView::new(
        inner,
        Box::new(|siv| {
            let options: Option<Result<TimezoneOptions, String>> =
                siv.call_on_name("timezone-options", |view: &mut FormView| {
                    let timezone = view
                        .get_value::<EditView, _>(1)
                        .ok_or("failed to retrieve timezone")?;

                    let kb_layout = view
                        .get_value::<EditView, _>(2)
                        .ok_or("failed to retrieve keyboard layout")?;

                    Ok(TimezoneOptions {
                        timezone,
                        kb_layout,
                    })
                });

            match options {
                Some(Ok(options)) => {
                    siv.with_user_data(|data: &mut InstallerData| {
                        data.options.timezone = options;
                    });

                    add_next_screen(siv, &password_dialog);
                }
                Some(Err(err)) => siv.add_layer(Dialog::info(format!("Invalid values: {err}"))),
                _ => siv.add_layer(Dialog::info("Invalid values")),
            }
        }),
    )
}

fn password_dialog(siv: &mut Cursive) -> InstallerView {
    let options = siv
        .user_data::<InstallerData>()
        .map(|data| data.options.password.clone())
        .unwrap_or_default();

    let inner = FormView::new()
        .child("Root password", EditView::new().secret())
        .child("Confirm root password", EditView::new().secret())
        .child("Administator email", EditView::new().content(options.email))
        .with_name("password-options");

    InstallerView::new(
        inner,
        Box::new(|siv| {
            let options = siv.call_on_name("password-options", |view: &mut FormView| {
                let root_password = view
                    .get_value::<EditView, _>(0)
                    .ok_or("failed to retrieve password")?;

                let confirm_password = view
                    .get_value::<EditView, _>(1)
                    .ok_or("failed to retrieve password confirmation")?;

                let email = view
                    .get_value::<EditView, _>(2)
                    .ok_or("failed to retrieve email")?;

                if root_password.len() < 5 {
                    Err("password too short")
                } else if root_password != confirm_password {
                    Err("passwords do not match")
                } else if email.ends_with(".invalid") {
                    Err("invalid email address")
                } else {
                    Ok(PasswordOptions {
                        root_password,
                        email,
                    })
                }
            });

            match options {
                Some(Ok(options)) => {
                    siv.with_user_data(|data: &mut InstallerData| {
                        data.options.password = options;
                    });

                    add_next_screen(siv, &network_dialog);
                }
                Some(Err(err)) => siv.add_layer(Dialog::info(format!("Invalid values: {err}"))),
                _ => siv.add_layer(Dialog::info("Invalid values")),
            }
        }),
    )
}

fn network_dialog(siv: &mut Cursive) -> InstallerView {
    let options = siv
        .user_data::<InstallerData>()
        .map(|data| data.options.network.clone())
        .unwrap_or_default();

    let inner = FormView::new()
        .child(
            "Management interface",
            SelectView::new().popup().with_all_str(vec!["eth0"]),
        )
        .child("Hostname (FQDN)", EditView::new().content(options.fqdn))
        .child(
            "IP address (CIDR)",
            CidrAddressEditView::new().content(options.address),
        )
        .child(
            "Gateway address",
            EditView::new().content(options.gateway.to_string()),
        )
        .child(
            "DNS server address",
            EditView::new().content(options.dns_server.to_string()),
        )
        .with_name("network-options");

    InstallerView::new(
        inner,
        Box::new(|siv| {
            let options = siv.call_on_name("network-options", |view: &mut FormView| {
                let ifname = view
                    .get_value::<SelectView, _>(0)
                    .ok_or("failed to retrieve management interface name")?;

                let fqdn = view
                    .get_value::<EditView, _>(1)
                    .ok_or("failed to retrieve host FQDN")?;

                let address = view
                    .get_value::<CidrAddressEditView, _>(2)
                    .ok_or("failed to retrieve host address")?;

                let gateway = view
                    .get_value::<EditView, _>(3)
                    .ok_or("failed to retrieve gateway address")?
                    .parse::<IpAddr>()
                    .map_err(|err| err.to_string())?;

                let dns_server = view
                    .get_value::<EditView, _>(3)
                    .ok_or("failed to retrieve DNS server address")?
                    .parse::<IpAddr>()
                    .map_err(|err| err.to_string())?;

                if address.addr().is_ipv4() != gateway.is_ipv4() {
                    Err("host and gateway IP address version must not differ".to_owned())
                } else if address.addr().is_ipv4() != dns_server.is_ipv4() {
                    Err("host and DNS IP address version must not differ".to_owned())
                } else if fqdn.chars().all(|c| c.is_ascii_digit()) {
                    // Not supported/allowed on Debian
                    Err("hostname cannot be purely numeric".to_owned())
                } else {
                    Ok(NetworkOptions {
                        ifname,
                        fqdn,
                        address,
                        gateway,
                        dns_server,
                    })
                }
            });

            match options {
                Some(Ok(options)) => {
                    siv.with_user_data(|data: &mut InstallerData| {
                        data.options.network = options;
                    });

                    add_next_screen(siv, &summary_dialog);
                }
                Some(Err(err)) => siv.add_layer(Dialog::info(format!("Invalid values: {err}"))),
                _ => siv.add_layer(Dialog::info("Invalid values")),
            }
        }),
    )
}

pub struct SummaryOption {
    name: &'static str,
    value: String,
}

impl SummaryOption {
    pub fn new<S: Into<String>>(name: &'static str, value: S) -> Self {
        Self {
            name,
            value: value.into(),
        }
    }
}

impl TableViewItem for SummaryOption {
    fn get_column(&self, name: &str) -> String {
        match name {
            "name" => self.name.to_owned(),
            "value" => self.value.clone(),
            _ => unreachable!(),
        }
    }
}

fn summary_dialog(siv: &mut Cursive) -> InstallerView {
    let options = siv
        .user_data::<InstallerData>()
        .map(|d| d.options.clone())
        .unwrap();

    let inner = LinearLayout::vertical()
        .child(PaddedView::lrtb(
            0,
            0,
            1,
            2,
            TableView::new()
                .columns(&[
                    ("name".to_owned(), "Option".to_owned()),
                    ("value".to_owned(), "Selected value".to_owned()),
                ])
                .items(options.to_summary()),
        ))
        .child(
            LinearLayout::horizontal()
                .child(DummyView.full_width())
                .child(Checkbox::new().with_name("reboot-after-install"))
                .child(
                    TextView::new(" Automatically reboot after successful installation").no_wrap(),
                )
                .child(DummyView.full_width()),
        )
        .child(PaddedView::lrtb(
            1,
            1,
            1,
            0,
            LinearLayout::horizontal()
                .child(abort_install_button())
                .child(DummyView.full_width())
                .child(Button::new("Previous", switch_to_prev_screen))
                .child(DummyView)
                .child(Button::new("Install", |_| {})),
        ));

    InstallerView::with_raw(inner)
}
