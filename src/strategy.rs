
use serde::{Deserialize};
use yaml_rust::Yaml;
use crate::error::{Result};
use crate::keypath::{KeyPathFuncs,KeyPath};
use regex::Regex;
use std::cell::{Ref,RefCell};

#[derive(PartialEq,Clone,Deserialize,Debug)]
pub struct Strategy {
    #[serde(default)]
    transform: Option<Transform>,
    #[serde(default)]
    filter: Option<FilterSpec>
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

#[derive(Deserialize,Clone,Debug)]
struct CachedRegex {
    regex: String,
    #[serde(skip)]
    re: RefCell<Option<regex::Regex>>
}

impl PartialEq for CachedRegex {
    fn eq(&self, other: &CachedRegex) -> bool {
        self.regex == other.regex
    }
}

impl CachedRegex {
    fn get_re(&self) -> Result<Ref<Option<regex::Regex>>> {
        {
            let mut bre = self.re.borrow_mut();
            if bre.is_none() {
                *bre = Some(Regex::new(&self.regex)?);
            }
        }
        Ok(self.re.borrow())
    }
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

//#[derive(Clone,Deserialize,Debug)]
#[derive(Deserialize,Clone,Debug,PartialEq)]
#[serde(untagged)]
enum ReplaceTransform {
    Value {
        path: String,
        value: YamlValue,
        with: Option<YamlValue>
    },
    Regex {
        path: String,
        #[serde(flatten)]
        regex: CachedRegex,
        with: String,
    }
}

#[derive(PartialEq,Clone,Deserialize,Debug)]
#[serde(untagged)]
enum FilterRule {
    PathRegex {
        #[serde(rename="pathRegex")]
        path_regex: String
    }
}

#[derive(PartialEq,Clone,Deserialize,Debug)]
struct FilterSpec {
    #[serde(default)]
    include: Vec<FilterRule>,
    #[serde(default)]
    exclude: Vec<FilterRule>
}

trait ConvYaml {
    fn equal_yaml(&self,y: &Yaml) -> bool;
    fn to_yaml(&self) -> Yaml;
}

impl ConvYaml for YamlValue {
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

impl ConvYaml for Option<YamlValue> {
    fn equal_yaml(&self,y: &Yaml) -> bool {
        match self {
            Some(val) => val.equal_yaml(y),
            None      => *y == Yaml::Null
        }
    }

    fn to_yaml(&self) -> Yaml {
        match self {
            Some(val) => val.to_yaml(),
            None      => Yaml::Null
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
            match replace {
                ReplaceTransform::Value{path,value,with} => {
                    let current = y.get_at_path(path.as_str())?;
                    if value.equal_yaml(current) {
                        y.set_at_path(path.as_str(),with.to_yaml())?;
                    }
                },
                ReplaceTransform::Regex{path,regex,with} => {
                    let current = y.get_at_path(path.as_str())?;
                    if let Some(strval) = current.as_str() {
                        let rep = regex.get_re()?.as_ref().unwrap().replace_all(strval, with);
                        let yrep = Yaml::String((*rep).to_string());
                        y.set_at_path(path.as_str(),yrep)?;
                    }
                }
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

impl FilterRule {
    fn accept(&self, path: &KeyPath) -> Result<bool> {
        match self {
            FilterRule::PathRegex{path_regex} => {
                let re = Regex::new(path_regex)?;
                Ok(re.is_match(path.to_string().as_str()))
            }
        }
    }
}

impl FilterSpec {
    fn accept(&self, path: &KeyPath) -> Result<bool> {
        let mut accepted = self.include.is_empty();
        for rule in &self.include {
            if rule.accept(path)? { 
                accepted = true;
                break;
            }
        }
        if accepted {
            for rule in &self.exclude {
                if rule.accept(path)? {
                    accepted = false;
                    break;
                }
            }
        }
        Ok(accepted)
    }
}

impl Strategy {
    pub fn from_str(text: &str) -> Result<Strategy> {
        Ok(serde_yaml::from_str(&text)?)
    }
    pub fn transform(&self,y: &mut Yaml, modified: bool) -> Result<()> {
        if let Some(transform) = &self.transform {
            let transforms = if modified {&transform.modified} else {&transform.original};
            for transform in transforms { transform.apply(y)?; }
            for transform in &transform.both { transform.apply(y)?; }
        }
        Ok(())
    }
    pub fn filter_accept(&self, path: &KeyPath) -> Result<bool> {
        match &self.filter {
            None => Ok(true),
            Some(filter) => filter.accept(path)
        }
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
        match &strategy.transform {
            Some(transform) => {
                assert_eq!(1,transform.original.len());
                if let TransformSelect::Value{path,value} = &transform.original[0].select[0] {
                    assert_eq!("kind",path);
                    assert_eq!(YamlValue::String("Deployment".to_string()),*value);
                } else {
                    panic!("Incorrect select type");
                }
                if let ReplaceTransform::Value{path:_,value,with:_} = &transform.original[0].replace[1] {
                    assert_eq!(YamlValue::Integer(1),*value);
                } else {
                    panic!("Transform type should be Value")
                }
            },
            None => panic!("Transform has not been set")
        }
    }
}