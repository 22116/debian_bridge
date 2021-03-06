use super::error::AppError;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt::Display,
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

pub type AppResult<T> = Result<T, AppError>;

const ICON_NAME_DEFAULT: &str = "debian_bridge_default.ico";

#[derive(Clone, Serialize, Deserialize)]
pub struct Icon {
    pub path: PathBuf,
}

impl Icon {
    pub fn new(path: &Path) -> Self {
        Icon {
            path: path.to_owned(),
        }
    }
}

impl Default for Icon {
    fn default() -> Self {
        let mut path = dirs::home_dir().unwrap();

        path.push(".icons");
        path.push(ICON_NAME_DEFAULT);

        Icon { path }
    }
}

#[derive(Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub enum Feature {
    Display,
    Sound,
    Notification,
    Devices,
    HomePersistent,
    Time,
}

impl Display for Feature {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Feature::Display => "Display",
                Feature::Sound => "Sound",
                Feature::Notification => "Notification",
                Feature::Devices => "Devices",
                Feature::HomePersistent => "Home persistent",
                Feature::Time => "Timezone",
            }
        )
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Program {
    name: String,
    pub path: PathBuf,
    pub settings: Vec<Feature>,
    pub icon: Option<Icon>,
    pub command: String,
    pub deps: Option<String>,
}

impl Program {
    pub fn get_name<T: Into<String>>(&self, prefix: T) -> String {
        format!("{}_{}", prefix.into(), self.name)
    }

    pub fn get_name_short(&self) -> String {
        self.name.to_owned()
    }

    pub fn new<T>(
        name: T,
        path: &Path,
        settings: &Vec<Feature>,
        icon: &Option<Icon>,
        cmd: &Option<String>,
        deps: &Option<String>,
    ) -> Self
    where
        T: Into<String>,
    {
        let name = name.into();

        Program {
            name: name.to_owned(),
            path: path.to_owned(),
            settings: settings.to_vec(),
            icon: icon.to_owned(),
            command: cmd.to_owned().unwrap_or(name),
            deps: deps.to_owned(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub programs: Vec<Program>,
}

impl Config {
    pub fn deserialize(path: &Path) -> AppResult<Self> {
        if !path.exists() {
            return File::create(path)
                .map(|_| {
                    let config = Config { programs: vec![] };

                    config.serialize(path);

                    config
                })
                .map_err(|err| AppError::File(err.to_string()));
        }

        let mut config_str = String::new();
        let config_file = File::open(path).map_err(|err| AppError::File(err.to_string()))?;

        let mut br = BufReader::new(config_file);

        br.read_to_string(&mut config_str)
            .map_err(|err| AppError::File(err.to_string()))?;

        if config_str.is_empty() {
            return Ok(Config { programs: vec![] });
        }

        serde_json::from_str(config_str.as_str()).map_err(|err| AppError::File(err.to_string()))
    }

    pub fn serialize(&self, path: &Path) -> AppResult<&Self> {
        let data = serde_json::to_string(&self).map_err(|err| AppError::File(err.to_string()))?;

        std::fs::write(path, data.as_bytes())
            .map(|_| self)
            .map_err(|err| AppError::File(err.to_string()))
    }

    pub fn push(&mut self, program: &Program) -> AppResult<&Self> {
        match self.programs.iter().find(|&x| x.name == program.name) {
            Some(elem) => {
                return Err(AppError::Program(
                    format!(
                        "Program with such name already exists '{}'. Remove it first or use a \
                         custom tag with -t (--tag) option",
                        program.name
                    )
                    .to_string(),
                ))
            }
            None => (),
        };

        self.programs.push(program.to_owned());
        Ok(self)
    }

    pub fn find<T: Into<String>>(&self, name: T) -> Option<(Program, usize)> {
        let name = name.into();
        let idx = self.programs.iter().position(|x| x.name == name)?;
        self.programs.get(idx).map(|p| (p.to_owned(), idx))
    }

    pub fn remove(&mut self, program: &Program) -> AppResult<&Self> {
        let program_idx = self
            .find(&program.name)
            .ok_or(AppError::Program(
                format!("Can't find a program '{}'", program.name).to_string(),
            ))?
            .1;

        self.programs.remove(program_idx);
        Ok(self)
    }

    pub fn clear(&mut self) -> &Self {
        self.programs = vec![];
        self
    }
}
