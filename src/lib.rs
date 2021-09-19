use anyhow::*;
use directories::ProjectDirs;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_yaml::{from_reader, Value};
use std::{
    ffi::OsStr,
    fs::{read, read_to_string, File},
    io::{BufRead, Read, Write},
    path::Path,
};
use tera::Tera;
use walkdir::WalkDir;

pub static DIRECTORIES: Lazy<ProjectDirs> =
    Lazy::new(|| directories::ProjectDirs::from("", "bluepython508", "generator").unwrap());

#[derive(Debug, Clone)]
struct TemplateDef {
    files: Vec<FileDef>,
    variables: Vec<VariableDef>,
}

impl TemplateDef {
    fn find_for_str(&self, s: &str) -> Option<&FileDef> {
        self.files.iter().find(|d| {
            d.sources.iter().any(|o| {
                let e = o.is_match(s);
                e
            })
        })
    }
}

#[derive(Debug, Clone)]
struct VariableDef {
    name: String,
    default: Option<String>,
}

#[derive(Debug, Clone)]
struct FileDef {
    sources: Vec<Regex>,
    template: bool,
    include: bool,
    rename: Option<String>,
}
fn parse_definition(def: impl Read) -> Result<TemplateDef> {
    let mut default_files_entry = vec![
        FileDef {
            sources: vec![Regex::new("^template.yml$").unwrap()],
            template: true,
            include: false,
            rename: None,
        },
        FileDef {
            sources: vec![Regex::new("^.git/").unwrap(), Regex::new("^.git$").unwrap()],
            include: false,
            template: true,
            rename: None,
        },
        FileDef {
            sources: vec![Regex::new(".*").unwrap()],
            include: true,
            template: true,
            rename: None,
        },
    ];
    let value: Value = from_reader(def).context("Invalid yaml in template definition")?;
    ensure!(
        value.is_mapping(),
        "Expected template definition to be mapping at top level"
    );
    let files = value
        .get("files")
        .map(|o| o.as_sequence().context("Expected `files` to be a sequence"))
        .transpose()?;
    let mut files = if let Some(files) = files {
        files
            .iter()
            .map(|o| match o {
                Value::String(s) => Ok(FileDef {
                    sources: vec![Regex::new(s).context("Expected valid regex")?],
                    template: true,
                    include: true,
                    rename: None,
                }),
                Value::Mapping(m) => Ok(FileDef {
                    sources: match m.get(&Value::String("sources".to_owned())) {
                        Some(Value::String(s)) => {
                            vec![Regex::new(s).context("Expected valid regex")?]
                        }
                        Some(Value::Sequence(s)) => s
                            .iter()
                            .map(|o| {
                                o.as_str()
                                    .map(|o| Regex::new(o).context("Expected a valid regex"))
                            })
                            .collect::<Option<Result<Vec<_>>>>()
                            .context("Expected a sequence of strings")??,
                        v => bail!(format!(
                            "Unexpected value {:?}, expected string or sequence of strings",
                            v
                        )),
                    },
                    template: m
                        .get(&Value::String("template".to_owned()))
                        .map(|o| o.as_bool().context("Expected `template` to be a boolean"))
                        .transpose()?
                        .unwrap_or(true),
                    include: m
                        .get(&Value::String("include".to_owned()))
                        .map(|o| o.as_bool().context("Expected `include` to be a boolean"))
                        .transpose()?
                        .unwrap_or(true),
                    rename: m
                        .get(&Value::String("rename".to_owned()))
                        .map(|o| o.as_str().context("Expected `rename` to be a string"))
                        .transpose()?
                        .map(|o| o.to_owned()),
                }),
                v => bail!(format!(
                    "Unexpected value {:?}, expected string or mapping",
                    v
                )),
            })
            .collect::<Result<_>>()?
    } else {
        vec![]
    };

    let variables = value
        .get("variables")
        .unwrap_or(&Value::Sequence(vec![]))
        .as_sequence()
        .context("Expected `variables` to be a sequence")?
        .into_iter()
        .map(|v| match v {
            Value::String(s) => Ok(VariableDef {
                name: s.to_owned(),
                default: None,
            }),
            Value::Mapping(m) => Ok(VariableDef {
                name: m
                    .get(&Value::String("name".to_owned()))
                    .context("Expected name for variable")?
                    .as_str()
                    .context("Expected variable name to be string")?
                    .to_string(),
                default: m
                    .get(&Value::String("default".to_owned()))
                    .map(|v| v.as_str().unwrap().to_owned()),
            }),
            v => bail!(format!(
                "Unexpected value {:?}, expected string or mapping",
                v
            )),
        })
        .collect::<Result<_>>()?;
    files.append(&mut default_files_entry);
    Ok(TemplateDef { files, variables })
}

fn prompt(context: &mut tera::Context, variable: &str) {
    print!("Variable {} missing - value? ", variable);
    std::io::stdout().flush().unwrap();
    context.insert(
        variable,
        &std::io::stdin().lock().lines().next().unwrap().unwrap(),
    );
}
pub fn generate(template: impl AsRef<Path>, destination: impl AsRef<Path>) -> Result<()> {
    let destination = destination.as_ref();
    let template = template.as_ref();
    let def = parse_definition(
        File::open(template.join("template.yml")).context("Template definition not found")?,
    )?;
    std::fs::create_dir_all(destination)?;
    let mut context = tera::Context::from_serialize(
        from_reader::<_, Value>(File::open(DIRECTORIES.config_dir().join("defaults.yml"))?)
            .context("While parsing default variables")?,
    )?;
    if let Some(s) = destination.file_name().and_then(OsStr::to_str) {
        context.insert("basename", s)
    }
    for var in &def.variables {
        if context.contains_key(&var.name) {
            continue;
        }
        if let Some(default) = &var.default {
            context.insert(&var.name, default)
        } else {
            prompt(&mut context, &var.name)
        }
    }
    for path in WalkDir::new(&template)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| {
            e.path()
                .strip_prefix(&template)
                .expect("Impossible as path guaranteed to be child of template")
                .to_str()
                .and_then(|o| def.find_for_str(o))
                .map(|o| o.include)
                .unwrap_or_default()
        })
        .filter_map(|f| f.ok())
        .map(|o| {
            o.path()
                .strip_prefix(&template)
                .expect("Impossible as path guaranteed to be child of template")
                .to_owned()
        })
    {
        let f = def
            .find_for_str(path.to_str().context("Filename is not a string")?)
            .context("Could not find a spec for file")?;
        let context = {
            let mut c = tera::Context::new();
            c.extend(context.clone());
            c.insert("file", &path);
            c
        };
        let input = template.join(&path);
        let new = destination.join(if let Some(rename) = &f.rename {
            Tera::one_off(rename, &context, false)?.into()
        } else {
            path.clone()
        });
        if input.is_dir() {
            std::fs::create_dir_all(&new).with_context(|| {
                format!("Could not create dir {}", new.display())
            })?;
        } else {
            let mut file = std::fs::File::create(&new).with_context(|| {
                format!(
                    "Destination {} already exists!",
                    new.display()
                )
            })?;
            file.write_all(&if f.template {
                Tera::one_off(
                    &read_to_string(&input)
                        .with_context(|| format!("Invalid UTF-8 in file {}", input.display()))?,
                    &context,
                    false,
                )?
                .into_bytes()
            } else {
                read(&input).with_context(|| format!("Failed to read file {}", input.display()))?
            })?;
        }
    }
    Ok(())
}
