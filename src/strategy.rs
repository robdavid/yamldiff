
use serde::{Deserialize};
use yaml_rust::Yaml;
use crate::error::{Result};
use crate::keypath::{KeyPathFuncs,KeyPath};
use regex::Regex;
use std::cell::{Ref,RefCell};
use std::ops::Deref;

#[derive(PartialEq,Clone,Deserialize,Debug)]
pub struct Strategy {
    #[serde(default)]
    transform: Option<Transform>,
    #[serde(default)]
    filter: Option<Filter>
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
struct Filter {
    path: Option<PathFilterSpec>,
    document: Option<DocumentFilterSpec>
}

#[derive(PartialEq,Clone,Deserialize,Debug)]
struct TransformSpec {
    #[serde(default)]
    select: Vec<PropertySelect>,
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
    fn get_re(&self) -> Result<ReRef> {
        {
            let mut bre = self.re.borrow_mut();
            if bre.is_none() {
                *bre = Some(Regex::new(&self.regex)?);
            }
        }
        Ok(ReRef{re_ref: self.re.borrow()})
    }
}

struct ReRef<'a> {
    re_ref: Ref<'a,Option<Regex>>
}

impl<'a> Deref for ReRef<'a> {
    type Target = Regex;
    fn deref(&self) -> &Regex {
        self.re_ref.as_ref().unwrap()
    }
}

#[derive(PartialEq,Clone,Deserialize,Debug)]
#[serde(untagged)]
enum PropertySelect {
    Value {
        path: String,
        value: YamlValue
    },
    Regex {
        path: String,
        #[serde(flatten)]
        regex: CachedRegex
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
enum PathFilterRule {
    PathRegex {
        #[serde(flatten)]
        regex: CachedRegex
    },
    PathName {
        name: String
    }
}

#[derive(PartialEq,Clone,Deserialize,Debug)]
#[serde(untagged)]
enum DocumentFilterRule {
    PropertySelect {
        select: Vec<PropertySelect>
    }
}

#[derive(PartialEq,Clone,Deserialize,Debug)]
struct PathFilterSpec {
    #[serde(default)]
    include: Vec<PathFilterRule>,
    #[serde(default)]
    exclude: Vec<PathFilterRule>
}

#[derive(PartialEq,Clone,Deserialize,Debug)]
struct DocumentFilterSpec {
    #[serde(default)]
    include: Vec<DocumentFilterRule>,
    #[serde(default)]
    exclude: Vec<DocumentFilterRule>
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

impl PropertySelect {
    fn accept(&self, y: &Yaml) -> Result<bool> {
        match self {
            PropertySelect::Value{path,value} => {
                match y.get_at_path(path.as_str()) {
                    Err(_) => Ok(false),
                    Ok(val) =>  Ok(value.equal_yaml(val))
                }
            }
            PropertySelect::Regex{path,regex} => {
                match y.get_at_path(path.as_str()) {
                    Err(_) => Ok(false),
                    Ok(val) => {
                        match val {
                            Yaml::String(text) => Ok(regex.get_re()?.is_match(text)),
                            _ => Ok(false)
                        }
                    }
                }
            }
        }
    }
}

impl TransformSpec {
    fn select(&self,y: &Yaml) -> Result<bool> {
        for select in &self.select {
            if !select.accept(y)? {
                return Ok(false)
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
                        let rep = regex.get_re()?.replace_all(strval, with);
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

impl PathFilterRule {
    fn accept(&self, path: &KeyPath) -> Result<bool> {
        match self {
            PathFilterRule::PathRegex{regex: path_regex} => {
                Ok(path_regex.get_re()?.is_match(path.to_string().as_str()))
            },
            PathFilterRule::PathName{name: path_name} => Ok(path.to_string().as_str() == path_name),
        }
    }
}

impl DocumentFilterRule {
    fn accept(&self, y: &Yaml) -> Result<bool> {
        match self {
            DocumentFilterRule::PropertySelect{select} => {
                for rule in select {
                    if !rule.accept(y)? {
                        return Ok(false)
                    }
                }
                Ok(true)
            },
        }
    }
}

fn include_exclude_filter<T,F: Fn(&T) -> Result<bool>>(include: &Vec<T>, exclude: &Vec<T>, predicate: F) -> Result<bool> {
    let mut accepted = include.is_empty();
    for item in include {
        if predicate(item)? { 
            accepted = true;
            break;
        }
    }
    if accepted {
        for item in exclude {
            if predicate(item)? {
                accepted = false;
                break;
            }
        }
    }
    Ok(accepted) 
}

impl PathFilterSpec {
    fn accept(&self, path: &KeyPath) -> Result<bool> {
        let predicate = |rule: &PathFilterRule| rule.accept(path);
        include_exclude_filter(&self.include, &self.exclude, predicate)
    }
}

impl DocumentFilterSpec {
    fn accept(&self, y: &Yaml) -> Result<bool> {
        let predicate = |rule: &DocumentFilterRule| rule.accept(y);
        include_exclude_filter(&self.include, &self.exclude, predicate)
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
            Some(filter) => match &filter.path {
                None => Ok(true),
                Some(path_filter) => path_filter.accept(path)
            }
        }
    }
    pub fn accept_document(&self, y: &Yaml) -> Result<bool> {
        match &self.filter {
            None => Ok(true),
            Some(filter) => match &filter.document {
                None => Ok(true),
                Some(doc_filter) => doc_filter.accept(y)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_deserialize_transform() {
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
                if let PropertySelect::Value{path,value} = &transform.original[0].select[0] {
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

    #[test]
    fn test_deserialize_path_filter() {
        let test_yaml = r#"
        filter:
          path:
            include:
              - regex: restring

        "#;
        let strategy = Strategy::from_str(test_yaml).map_err(|e| e.to_string()).unwrap();
        let include = strategy.filter.unwrap().path.unwrap().include;
        assert_eq!(include.len(),1);
        if let PathFilterRule::PathRegex{regex: path_regex} = &include[0] {
            assert_eq!(path_regex.regex,"restring");
        } else {
            panic!("Expected path regex type")
        }
    }   

    #[test]
    fn test_deserialize_document_filter() {
        let test_yaml = r#"
        filter:
          document:
            include:
              - select:
                - path: kind
                  value: Service

        "#;
        let strategy = Strategy::from_str(test_yaml).map_err(|e| e.to_string()).unwrap();
        let include = strategy.filter.unwrap().document.unwrap().include;
        assert_eq!(include.len(),1);
        let DocumentFilterRule::PropertySelect{select} = &include[0];
        assert_eq!(select.len(),1);
        if let PropertySelect::Value{path,value} = &select[0] {
            assert_eq!(path,"kind");
            assert_eq!(*value,YamlValue::String("Service".to_string()));
        } else {
            panic!("Expected path value select")
        }
    }   

}