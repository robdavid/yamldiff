use serde::{Deserialize};
use yaml_rust::Yaml;
use crate::error::{Result};
use crate::keypath::{KeyPathFuncs};
use regex::Regex;

#[derive(PartialEq,Clone,Deserialize,Debug)]
pub struct Strategy {
    transform: Transform
}


#[derive(PartialEq,Clone,Deserialize,Debug)]
struct Transform {
    #[serde(default)]
    original: Vec<TransformSpec>,
    #[serde(default)]
    modified: Vec<TransformSpec>,
    #[serde(default)]
    both: Vec<TransformSpec>
}

#[derive(PartialEq,Clone,Deserialize,Debug)]
struct TransformSpec {
    #[serde(default)]
    select: Vec<TransformSelect>,
    #[serde(default)]
    replace: Vec<ReplaceTransform>,
    #[serde(default)]
    set: Vec<YamlPathAndValue>,
    #[serde(default)]
    drop: bool
}

#[derive(PartialEq,Clone,Deserialize,Debug)]
#[serde(untagged)]
enum YamlValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool)
}

#[derive(PartialEq,Clone,Deserialize,Debug)]
struct YamlPathAndValue {
    path: String,
    value: YamlValue
}

#[derive(PartialEq,Clone,Deserialize,Debug)]
#[serde(untagged)]
enum TransformSelect {
    Value {
        path: String,
        value: YamlValue
    },
    Regex {
        path: String,
        regex: String
    }
}

#[derive(PartialEq,Clone,Deserialize,Debug)]
struct ReplaceTransform {
    path: String,
    value: YamlValue,
    with: YamlValue
}

impl YamlValue {
    fn equal_yaml(&self,y: &Yaml) -> bool {
        match self {
            YamlValue::String(s1) => {
                if let Yaml::String(s2) = y {
                    *s1 == *s2
                } else {
                    false
                }
            },
            YamlValue::Boolean(b) => *y == Yaml::Boolean(*b),
            YamlValue::Float(f)   => *y == Yaml::Real(f.to_string()),
            YamlValue::Integer(i) => *y == Yaml::Integer(*i)
        }
    }

    fn to_yaml(&self) -> Yaml {
        match self {
            YamlValue::String(s)  => Yaml::String(s.clone()),
            YamlValue::Boolean(b) => Yaml::Boolean(*b),
            YamlValue::Float(f)   => Yaml::Real(f.to_string()),
            YamlValue::Integer(i) => Yaml::Integer(*i)
        }
    }
}

impl TransformSpec {
    fn select(&self,y: &Yaml) -> Result<bool> {
        for select in &self.select {
            match select {
                TransformSelect::Value {path,value} => {
                    match y.get_at_path(path.as_str()) {
                        Err(_) => { return Ok(false); }
                        Ok(val) => {
                            if !value.equal_yaml(val) {
                                return Ok(false);
                            }
                        }
                    }
                }
                TransformSelect::Regex {path,regex} => {
                    match y.get_at_path(path.as_str()) {
                        Err(_) => { return Ok(false); }
                        Ok(val) => {
                            let re = Regex::new(regex)?;
                            match val {
                                Yaml::String(text) => {
                                    if !re.is_match(text) { return Ok(false);}
                                }
                                _ => { return Ok(false) }
                            }
                        }
                    }
                }
            }
        }
        Ok(true)
    }
    fn apply_replace(&self,y: &mut Yaml) -> Result<()> {
        for replace in &self.replace {
            let current = y.get_at_path(replace.path.as_str())?;
            if replace.value.equal_yaml(current) {
                y.set_at_path(replace.path.as_str(),replace.with.to_yaml())?;
            }
        }   
        Ok(())
    }
    fn apply_set(&self,y: &mut Yaml) -> Result<()> {
        for set in &self.set {
            y.set_at_path(set.path.as_str(),set.value.to_yaml())?;
        }
        Ok(())
    }
    fn apply_drop(&self, y: &mut Yaml) -> bool{
        if self.drop {
            *y = Yaml::Null;
            true
        } else {
            false
        }
    }
    fn apply(&self, y: &mut Yaml) -> Result<()> {
        if self.select(y)? {
            if !self.apply_drop(y) {
                self.apply_replace(y)?;
                self.apply_set(y)?;
            }
        }
        Ok(())
    }
}

impl Strategy {
    pub fn from_str(text: &str) -> Result<Strategy> {
        Ok(serde_yaml::from_str(&text)?)
    }
    pub fn transform(&self,y: &mut Yaml, modified: bool) -> Result<()> {
        let transforms = if modified {&self.transform.modified} else {&self.transform.original};
        for transform in transforms { transform.apply(y)?; }
        for transform in &self.transform.both { transform.apply(y)?; }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_deserialize() {
        let test_yaml = r#"
        transform:
            original:
            - select:
                - path: "kind"
                  value: "Deployment"
              replace:
                - path: "metadata.name"
                  value: "origname"
                  with: "newname"
                - path: "spec.replicas"
                  value: 1
                  with: 2
              set:
                - path: "metadata.namespace"
                  value: "production"
        "#;
        let strategy = Strategy::from_str(test_yaml).map_err(|e| e.to_string()).unwrap();
        assert_eq!(1,strategy.transform.original.len());
        if let TransformSelect::Value{path,value} = &strategy.transform.original[0].select[0] {
            assert_eq!("kind",path);
            assert_eq!(YamlValue::String("Deployment".to_string()),*value);
        } else {
            panic!("Incorrect select type");
        }
        assert_eq!(YamlValue::Integer(1),strategy.transform.original[0].replace[1].value);
    }
}