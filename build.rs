#[macro_use]
extern crate clap;
extern crate xdg;
extern crate dirs;

use clap::{App, Shell};
use std::path::Path;
use std::error::Error;

fn main() {
    source_bashrc().unwrap_or_else(|err| {
        println!("Can not install autocompletion: {}", err.to_string());
        println!("You can do it manually by including generated {}.<shell-extension> to your profile config", env!("CARGO_PKG_NAME"));
        println!("If there are no such file, contact me pls.");
    });
}

fn source_bashrc() -> Result<(), Box<dyn Error>> {
    let config_path = xdg::BaseDirectories::with_prefix(env!("CARGO_PKG_NAME"))?
        .get_config_home();
    let yaml = load_yaml!("./config/cli.yaml");
    let matches = App::from_yaml(yaml)
        .gen_completions(
            env!("CARGO_PKG_NAME"),
            Shell::Bash,
            config_path.as_os_str().to_owned()
        );

    let source_str = format!(
        "\nsource {}{}.bash",
        config_path.to_str().ok_or("Can't get a config path")?,
        env!("CARGO_PKG_NAME")
    );
    let mut bashrc = dirs::home_dir().ok_or("Can't get a home path")?;
    bashrc.push(".bashrc");
    let mut bashrc_str = std::fs::read_to_string(bashrc.as_path())?;

    if !bashrc_str.contains(&source_str) {
        bashrc_str.push_str(source_str.as_str());
        std::fs::write(bashrc, bashrc_str)?;
    }

    Ok(())
}