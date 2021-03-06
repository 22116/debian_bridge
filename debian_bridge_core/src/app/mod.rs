mod config;
mod deb;
mod docker;
pub mod error;
mod util;

use crate::System;
use colorful::{core::StrMarker, Color, Colorful};
pub use config::{Config, Feature, Icon, Program};
use deb::Deb;
use docker::DockerFacade;
use error::AppError;
use serde_json::to_string;
use shiplift::Docker;
use std::{
    collections::HashMap,
    error::Error,
    fmt::{Display, Formatter},
    net::IpAddr,
    path::{Path, PathBuf},
};

type AppResult<T> = Result<T, AppError>;

pub struct FeaturesList {
    list: HashMap<Feature, bool>,
}

impl FeaturesList {
    fn new(system: &System) -> Self {
        let mut list = HashMap::new();

        list.insert(Feature::Display, system.wm.is_some());
        list.insert(Feature::Sound, system.sd.is_some());
        list.insert(Feature::Devices, true);
        list.insert(Feature::Notification, true);
        list.insert(Feature::Time, true);
        list.insert(Feature::HomePersistent, true);

        Self { list }
    }

    fn validate(&self, settings: &Vec<Feature>) -> bool {
        settings
            .iter()
            .try_for_each(|f| {
                if !*self.list.get(f).ok_or(())? {
                    Err(())
                } else {
                    Ok(())
                }
            })
            .is_ok()
    }
}

impl Display for FeaturesList {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        writeln!(f, "\n");

        for (feature, available) in &self.list {
            writeln!(
                f,
                "\t{:<15} ===> {}",
                format!("{}", feature),
                match available {
                    true => "available".color(Color::Green),
                    false => "unavailable".color(Color::Red),
                }
            );
        }

        Ok(())
    }
}

/// Main structure to run application
///
/// # Example
/// ```no_run
/// use debian_bridge_core::{App, Config, Docker, System};
/// use std::path::Path;
///
/// let docker = Docker::new();
/// let config = Config::deserialize(Path::new("./cfg")).unwrap();
/// let system = System::try_new(&docker).unwrap();
/// let mut app = App::new("debian_bridge", "foo_package", Path::new("./cache"), &config, &system, &docker);
/// //...
/// app.save(Path::new("./cfg")).unwrap();
/// ```
pub struct App<'a> {
    package_name: String,
    prefix: String,
    cache_path: PathBuf,
    config: Config,
    docker: DockerFacade<'a>,
    pub features: FeaturesList,
}

impl<'a> App<'a> {
    pub fn list(&self) -> Vec<String> {
        self.config
            .programs
            .iter()
            .map(|program| (&program).get_name_short().to_owned())
            .collect::<Vec<String>>()
            .to_vec()
    }

    /// Removes existed program
    ///
    /// # Example
    /// ```no_run
    /// # use debian_bridge_core::{App, Config, Docker, System};
    /// # use std::path::Path;
    /// #
    /// # let docker = Docker::new();
    /// # let config = Config::deserialize(Path::new("./cfg")).unwrap();
    /// # let system = System::try_new(&docker).unwrap();
    /// let mut app = App::new("debian_bridge", "foo_package", Path::new("./cache"), &config, &system, &docker);
    /// app.remove("foo-program").unwrap();
    /// app.save(Path::new("./cfg")).unwrap();
    /// ```
    pub fn remove<T: Into<String>>(&mut self, program: T) -> AppResult<&Self> {
        let program = self
            .config
            .find(program.into())
            .ok_or(AppError::Program("Input program doesn't exist".to_str()))?
            .0;

        match self.docker.delete(&program) {
            Ok(_) => (),
            Err(AppError::DockerStatus(404)) => (),
            Err(err) => return Err(err),
        };
        self.config.remove(&program)?;

        if let Some(_) = program.icon {
            let mut path = dirs::desktop_dir().unwrap();
            let name = format!("{}.desktop", program.get_name_short());

            path.push(name);

            std::fs::remove_file(path).unwrap_or_else(|err| {
                error!("Can't remove an entry file: '{}'", err.to_string());
                ()
            });
        }

        Ok(self)
    }

    /// Creates new program
    ///
    /// # Example
    /// ```no_run
    /// # use debian_bridge_core::{App, Config, Docker, System, Feature};
    /// # use std::path::Path;
    /// #
    /// # let docker = Docker::new();
    /// # let config = Config::deserialize(Path::new("./cfg")).unwrap();
    /// # let system = System::try_new(&docker).unwrap();
    /// let mut app = App::new("debian_bridge", "foo_package", Path::new("./cache"), &config, &system, &docker);
    /// app.create(Path::new("./package.deb"), &vec![Feature::Display], &None, &None, &None).unwrap();
    /// app.save(Path::new("./cfg")).unwrap();
    /// ```
    pub fn create(
        &mut self,
        app_path: &Path,
        settings: &Vec<Feature>,
        icon: &Option<Icon>,
        cmd: &Option<String>,
        deps: &Option<String>,
    ) -> AppResult<&Self> {
        if !self.features.validate(&settings) {
            return Err(AppError::Program(
                "You have set unavailable feature".to_string(),
            ));
        }

        let deb = Deb::try_new(app_path)?;
        let program = Program::new(&deb.package, &app_path, &settings, &icon, &cmd, &deps);
        let mut app_tmp_path = self.cache_path.to_owned();

        std::fs::create_dir_all(&app_tmp_path).map_err(|err| AppError::File(err.to_string()))?;
        app_tmp_path.push(Path::new("tmp.deb"));
        std::fs::copy(app_path, &app_tmp_path).map_err(|err| AppError::File(err.to_string()))?;

        let mut dockerfile = util::gen_dockerfile(&deb, &program)?;

        debug!("Generated dockerfile:\n{}", dockerfile);

        let mut dockerfile_path = self.cache_path.to_owned();
        dockerfile_path.push(Path::new("Dockerfile"));

        std::fs::write(&dockerfile_path, dockerfile)
            .map_err(|err| AppError::File(err.to_string()))?;

        self.config.push(&program)?;
        self.docker.create(&deb.package)?;

        std::fs::remove_file(&dockerfile_path).map_err(|err| AppError::File(err.to_string()))?;
        std::fs::remove_file(&app_tmp_path).map_err(|err| AppError::File(err.to_string()))?;

        if let Some(icon) = &icon {
            self.create_entry(&icon, &deb).unwrap_or_else(|err| {
                warn!("{}", err.to_string());
                &self
            });
        }

        Ok(self)
    }

    /// Runs existed program
    ///
    /// # Example
    /// ```no_run
    /// # use debian_bridge_core::{App, Config, Docker, System};
    /// # use std::path::Path;
    /// #
    /// # let docker = Docker::new();
    /// # let config = Config::deserialize(Path::new("./cfg")).unwrap();
    /// # let system = System::try_new(&docker).unwrap();
    /// let mut app = App::new("debian_bridge", "foo_package", Path::new("./cache"), &config, &system, &docker);
    /// app.run("foo_program").unwrap();
    /// ```
    pub fn run<T: Into<String>>(&self, program: T) -> AppResult<&Self> {
        let program = self
            .config
            .find(program)
            .ok_or(AppError::Program("Program not found".to_string()))?
            .0;

        self.docker.run(&program)?;
        Ok(self)
    }

    /// Saves current application configuration
    ///
    /// # Example
    /// ```no_run
    /// # use debian_bridge_core::{App, Config, Docker, System};
    /// # use std::path::Path;
    /// #
    /// # let docker = Docker::new();
    /// # let config = Config::deserialize(Path::new("./cfg")).unwrap();
    /// # let system = System::try_new(&docker).unwrap();
    /// let mut app = App::new("debian_bridge", "foo_package", Path::new("./cache"), &config, &system, &docker);
    /// app.save(Path::new("./cfg_new")).unwrap();
    /// ```
    pub fn save(&self, path: &Path) -> AppResult<&Self> {
        self.config.serialize(path)?;
        debug!("Config updated");
        Ok(self)
    }

    /// Creates new App instance
    ///
    /// # Example
    /// ```no_run
    /// # use debian_bridge_core::{App, Config, Docker, System};
    /// # use std::path::Path;
    /// #
    /// # let docker = Docker::new();
    /// # let config = Config::deserialize(Path::new("./cfg")).unwrap();
    /// # let system = System::try_new(&docker).unwrap();
    /// let mut app = App::new("debian_bridge", "foo_package", Path::new("./cache"), &config, &system, &docker);
    /// ```
    pub fn new<T: Into<String>, S: Into<String>>(
        package_name: T,
        prefix: S,
        cache_path: &Path,
        config: &Config,
        system: &'a System,
        docker: &'a Docker,
    ) -> Self {
        let package_name = package_name.into();
        let prefix = prefix.into();

        App {
            package_name,
            prefix: prefix.to_owned(),
            config: config.to_owned(),
            docker: DockerFacade::new(docker, system, prefix, cache_path),
            cache_path: cache_path.to_owned(),
            features: FeaturesList::new(&system),
        }
    }

    fn create_entry(&self, icon: &Icon, deb: &Deb) -> AppResult<&Self> {
        let entry = util::gen_desktop_entry(
            &self.package_name,
            &deb.package,
            &deb.description
                .to_owned()
                .unwrap_or("Application".to_string()),
            &icon.path,
        );

        let entry = entry.map_err(|err| AppError::File(err.to_string()))?;
        let mut path = dirs::desktop_dir().unwrap();

        debug!(
            "Generated new entry in '{}':\n{}",
            path.to_str().unwrap(),
            entry
        );

        if !path.exists() {
            std::fs::create_dir(&path).map_err(|err| AppError::File(err.to_string()))?;
        }

        path.push(format!("{}.desktop", deb.package));

        std::fs::write(path, entry).map_err(|err| AppError::File(err.to_string()))?;

        Ok(self)
    }
}
