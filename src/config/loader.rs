// Configuration file loading and creation

use super::types::Config;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Get the path to the configuration file
pub fn get_config_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("p2pong");

    // Create config directory if it doesn't exist
    fs::create_dir_all(&path).ok();

    path.push("config.toml");
    path
}

/// Load configuration from file, or create default if it doesn't exist
pub fn load_config() -> Result<Config, io::Error> {
    let config_path = get_config_path();

    if config_path.exists() {
        let contents = fs::read_to_string(&config_path)?;
        match toml::from_str(&contents) {
            Ok(config) => Ok(config),
            Err(e) => {
                eprintln!("Warning: Failed to parse config file: {}", e);
                eprintln!("Using default configuration");
                Ok(Config::default())
            }
        }
    } else {
        // Create default config file
        create_default_config(&config_path)?;
        Ok(Config::default())
    }
}

/// Create a default configuration file with helpful comments
pub fn create_default_config(path: &Path) -> Result<(), io::Error> {
    let config = Config::default();
    let toml_string =
        toml::to_string_pretty(&config).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    // Add helpful header comments
    let commented_toml = format!(
        "# P2Pong Configuration File\n\
         # Edit this file to customize game behavior\n\
         # After editing, restart the game for changes to take effect\n\
         #\n\
         # Key binding format: Use \"Up\", \"Down\", \"Left\", \"Right\", \"Enter\", \"Esc\"\n\
         #                     or single characters like \"W\", \"S\", \"Q\", etc.\n\
         #\n\
         # Colors: RGB values from 0-255\n\
         #\n\
         # AI difficulties: \"easy\", \"medium\", \"hard\"\n\n\
         {}",
        toml_string
    );

    fs::write(path, commented_toml)?;
    println!("Created default config file at: {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_serialization() {
        let config = Config::default();
        let toml_string = toml::to_string_pretty(&config).unwrap();

        // Should round-trip cleanly — parsed values must match the original defaults
        let parsed: Config = toml::from_str(&toml_string).unwrap();

        assert_eq!(
            parsed.physics.ball_initial_speed,
            config.physics.ball_initial_speed
        );
        assert_eq!(parsed.physics.paddle_height, config.physics.paddle_height);
        assert_eq!(
            parsed.keybindings.left_paddle_up,
            config.keybindings.left_paddle_up
        );
        assert_eq!(parsed.display.target_fps, config.display.target_fps);
        assert_eq!(parsed.ai.difficulty, config.ai.difficulty);
    }

    #[test]
    fn test_partial_config_with_defaults() {
        // Should be able to parse partial config with #[serde(default)]
        let partial_toml = r#"
            [physics]
            ball_initial_speed = 500.0
        "#;

        let config: Config = toml::from_str(partial_toml).unwrap();

        // Custom value
        assert_eq!(config.physics.ball_initial_speed, 500.0);

        // Default values should still be there
        assert_eq!(config.physics.paddle_height, 90.0);
        assert_eq!(config.keybindings.left_paddle_up, "W");
    }
}
