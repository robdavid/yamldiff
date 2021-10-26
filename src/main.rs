extern crate yaml_rust;
//#[macro_use]
extern crate clap;
#[macro_use]
extern crate error_chain;
extern crate diffy;
extern crate ansi_colors;
extern crate linked_hash_map;
extern crate regex;
extern crate serde;

mod keypath;
mod error;

use yaml_rust::{YamlLoader,Yaml,yaml};
use clap::Parser;
use error_chain::ChainedError;
use linked_hash_map::LinkedHashMap;
use std::collections::HashMap;
use std::fmt::{Formatter,Display};
use std::rc::Rc;
use std::cmp::max;
use std::{fs,fmt};
use std::process::exit;
use diffy::{create_patch,PatchFormatter};
use ansi_colors::*;
use regex::Regex;
use serde::{Deserialize};
use keypath::{ItemKey,KeyPath};
use error::{ErrorKind,Result,ResultExt};


#[derive(Parser)]
struct Opts {
    file1: String,
    file2: String,
    #[clap(short,long,about="Compare kubernetes yaml documents")]
    k8s: bool,
    #[clap(short,long,about="Don't produce coloured output")]
    no_colour: bool,
    #[clap(short('x'),long,multiple_occurrences(true),about="Exclude YAML document paths matching regex")]
    exclude: Vec<String>,
    #[clap(short('f'),long,about="Difference strategy file")]
    strategy: Option<String>
}

impl Opts {
    fn exclude_regex(&self) -> Result<Vec<Regex>> {
        let mut result = Vec::<Regex>::new();
        for excl in &self.exclude {
            result.push(Regex::new(excl)?)
        }
        Ok(result)
    }
}

#[derive(Deserialize)]
struct Strategy {
    mapping: Mapping
}

#[derive(Deserialize)]
struct Mapping {
    document: MapDocument
}

#[derive(Deserialize)]
struct MapDocument {
    k8s: Vec<MapDocK8s>
}

#[derive(Deserialize)]
struct MapDocK8s {
    #[serde(rename="groupVersion")]
    group_version: Option<String>,
    kind: Option<String>,
    #[serde(default)]
    rename: HashMap<String,MapRename>
}

#[derive(Deserialize)]
struct MapRename {
    from: String,
    to: String
}


/** A string struct that can hold either a borrowed reference or String value */
enum LzyStr<'a> {
    Ref(&'a str),
    Val(String)
}

/** Convert from String */
impl From<String> for LzyStr<'static> {
    fn from(s: String) -> LzyStr<'static> {
        LzyStr::Val(s)
    }
}

/** Convert from &str */
impl<'a> From<&'a str> for LzyStr<'a> {
    fn from(s: &'a str) -> LzyStr<'a> {
        LzyStr::Ref(s)
    }
}

/** Display the string */
impl<'a> Display for LzyStr<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            LzyStr::Ref(s) => write!(f,"{}",s),
            LzyStr::Val(s) => write!(f,"{}",s)
        }
    }
}

/** Kubernetes metatdata - group, version and kind */
#[derive(PartialEq,Eq,Hash,Debug,Clone)]
struct GRK {
    api_version: String,
    kind: String
}

impl Display for GRK {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f,"{},{}",self.api_version,self.kind)
    }
}

/** Kubernetes metadata - group, version kind plus name & namespace */
#[derive(PartialEq,Eq,Hash,Debug,Clone)]
struct K8SMeta {
    grv: GRK,
    name: String,
    namespace: Option<String>
}

impl Display for K8SMeta {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self.namespace {
            None     => write!(f,"{},{}",self.grv,self.name),
            Some(ns) => write!(f,"{},{}/{}",self.grv,self.name,ns)
        }
    }
}

/** 
 * Index key for multiple document YAML files - either by 
 * position or Kubernetes metadata 
 */
#[derive(PartialEq,Eq,Hash,Debug,Clone)]
enum DocKey {
    Position(i32),
    K8S(K8SMeta)
}

impl Display for DocKey {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            DocKey::Position(n) => write!(f,"[{}]",n),
            DocKey::K8S(m)      => write!(f,"{}",m)
        }
    }
}

type Documents = LinkedHashMap<DocKey,Yaml>;

#[derive(PartialEq,Clone)]
struct Location<'a> {
    fname: &'a str,
    doc: Rc<DocKey>,
    path: KeyPath,
}

impl<'a> Display for Location<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f,"{}: {} {}",self.fname,self.doc,self.path)
    }
}

impl<'a> Location<'a> {
    fn new(fname: &'a str, doc: Rc<DocKey>, path: KeyPath) -> Location<'a> {
        Location{fname,doc,path}
    }
    fn parent(&self) -> Option<Location<'a>> {
        if self.path.0.is_empty() {
            None
        } else {
            let mut newvec = self.path.0.clone();
            newvec.pop();
            Some(Location{fname: self.fname, doc: self.doc.clone(), path: KeyPath(newvec)})
        }
    }
}

struct LocationAndValue<'a> {
    loc: Location<'a>,
    value: Yaml
}

impl<'a> LocationAndValue<'a> {
    fn new(fname: &'a str, doc: Rc<DocKey>, path: KeyPath, value: &Yaml) -> LocationAndValue<'a> {
        LocationAndValue{loc: Location::new(fname,doc,path),value: value.clone()}
    }
}

impl<'a> Display for LocationAndValue<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f,"{} = {:?}",self.loc,self.value)
    }
}

enum Diff<'a> {
    Add(LocationAndValue<'a>),
    Remove(LocationAndValue<'a>),
    Differ(LocationAndValue<'a>,LocationAndValue<'a>)
}

impl<'a> Diff<'a> {
    fn add(fname: &'a str, doc: Rc<DocKey>, path: KeyPath, value: &Yaml) -> Diff<'a> {
        Diff::Add(LocationAndValue::new(fname,doc,path,value))
    }
    fn remove(fname: &'a str, doc: Rc<DocKey>, path: KeyPath, value: &Yaml) -> Diff<'a> {
        Diff::Remove(LocationAndValue::new(fname,doc,path,value))
    }
    fn differ(fname1: &'a str, fname2: &'a str, doc: Rc<DocKey>, path: KeyPath, value1: &Yaml, value2: &Yaml) -> Diff<'a> {
        Diff::Differ(LocationAndValue::new(fname1,doc.clone(),path.clone(),value1),LocationAndValue::new(fname2,doc,path,value2))
    }
    fn key_path(&self) -> &KeyPath {
        match self {
            Diff::Add(lav) => &lav.loc.path,
            Diff::Remove(lav) => &lav.loc.path,
            Diff::Differ(lav1,_) => &lav1.loc.path,
        }
    }
}

fn load_file(fname: &str) -> Result<Vec<Yaml>> {
    let yaml_text = fs::read_to_string(fname).chain_err(|| format!("while reading {}",fname))?;
    Ok(YamlLoader::load_from_str(&yaml_text)?)
}


trait YamlFuncs {
    fn str_result(&self, key: &str) -> Result<&str>;
    fn string_result(&self, key: &str) -> Result<String>;
    fn is_hash(&self) -> bool;
    fn set_value(&mut self, path: &[&str], value: Yaml);
}

impl YamlFuncs for Yaml {
    fn str_result(&self, key: &str) -> Result<&str> {
        let val = self[key].as_str();
        val.ok_or(ErrorKind::KeyNotFound(key.to_string()).into())
    }
    fn string_result(&self,key: &str) -> Result<String> {
        Ok(self.str_result(key)?.to_string())
    }
    fn is_hash(&self) -> bool {
        match self {
            Yaml::Hash(_) => true,
            _ =>             false
        }
    }
    fn set_value(&mut self, path: &[&str], value: Yaml) {
        let mut current: &mut Yaml = self;
        for i in 0..path.len() {
            if let Yaml::Hash(h) = current {
                if i == path.len()-1 {
                    h.insert(Yaml::String(path[i].to_string()),value.clone());
                    break;
                } else {
                    current = &mut h[&Yaml::from_str(path[i])];
                }
            } else {
                break;
            }
        }
    }
}

impl MapDocument {
    fn doc_mapping(&self, key: DocKey, yaml: &mut Yaml) -> Result<DocKey> {
        match key {
            DocKey::K8S(mut meta) => {
                for mapdoc in &self.k8s {
                    if mapdoc.group_version.as_ref() != Some(&meta.grv.api_version) {
                        continue;
                    }
                    if mapdoc.kind.as_ref() != Some(&meta.grv.kind) {
                        continue;
                    }
                    for (item,rename) in &mapdoc.rename {
                        match item.as_str() {
                            "name" => {
                                if meta.name == rename.from {
                                    meta.name = rename.to.clone();
                                    yaml.set_value(&["metadata","name"],Yaml::from_str(&rename.to))
                                }
                            }
                            _ => return Err(ErrorKind::UnknownRenameField(item.clone()).into())
                        }
                    }
                }
                Ok(DocKey::K8S(meta))
            },
            _ => Ok(key)
        }
    }
}

fn index(docs: Vec<Yaml>,opts: &Opts) -> Result<Documents> {
    let map_doc = match &opts.strategy {
        Some(fname) => {
            let yaml = fs::read_to_string(fname)?;
            let strategy : Strategy = serde_yaml::from_str(&yaml)?;
            Some(strategy.mapping.document)
        },
        _ => None
    };
    let mut result = Documents::new();
    if opts.k8s {
        for mut yaml in docs {
            if yaml.is_null() { continue; }
            let api_version = yaml.string_result("apiVersion")?;
            let kind = yaml.string_result("kind")?;
            let name = yaml["metadata"].string_result("name")?;
            if let Yaml::Hash(ref mut md) = &mut yaml {
                md.insert(Yaml::String("name".to_string()),Yaml::String("myvalue".to_string()));
            }
            
            let namespace = yaml["metadata"]["namespace"].as_str().map(String::from);
            let mut key = DocKey::K8S(K8SMeta{name,namespace,grv:GRK{api_version,kind}});
            match &map_doc {
                Some(md) => { key = md.doc_mapping(key,&mut yaml)?; }
                None => ()
            }
            result.insert(key,yaml);
        }
    } else {
        let mut index = 0;
        for yaml in docs {
            result.insert(DocKey::Position(index),yaml);
            index+=1;
        }
    }
    Ok(result)
}

type Diffs<'a> = Vec<Diff<'a>>;

fn recurse_array_diffs<'a>(opts: &'a Opts, dockey: Rc<DocKey>, diffs: &mut Diffs<'a>, path: KeyPath, y1: &Yaml, y2: &Yaml) {
    let empty = Vec::<Yaml>::new();
    let null_yaml = Yaml::Null;
    let arr1 = y1.as_vec().unwrap_or(&empty);
    let arr2 = y2.as_vec().unwrap_or(&empty);
    let max_len = max(arr1.len(),arr2.len());
    for i in 0..max_len {
        let v1 = if i < arr1.len() { &arr1[i] } else { &null_yaml };
        let v2 = if i < arr2.len() { &arr2[i] } else { &null_yaml };
        recurse_diffs(opts, dockey.clone(), diffs, path.push(ItemKey::Index(i)), v1, v2);
    }
    if !y1.is_array() {
        recurse_diffs(opts, dockey, diffs, path, y1, &null_yaml);
    } else if !y2.is_array() {
        recurse_diffs(opts, dockey, diffs, path, &null_yaml,y2);
    }
}

fn recurse_hash_diffs<'a>(opts: &'a Opts, dockey: Rc<DocKey>, diffs: &mut Diffs<'a>, path: KeyPath, y1: &Yaml, y2: &Yaml) {
    let empty = yaml::Hash::new();
    let null_yaml = Yaml::Null;
    let hash1 = y1.as_hash().unwrap_or(&empty);
    let hash2 = y2.as_hash().unwrap_or(&empty);
    for key in hash1.keys() {
        let v1 = &hash1[key];
        let v2 = if hash2.contains_key(key) { &hash2[key] } else { &null_yaml };
        let next_key = ItemKey::Key(key.as_str().unwrap().to_string());
        recurse_diffs(opts, dockey.clone(), diffs, path.push(next_key), v1, v2);
    }
    for key in hash2.keys() {
        let v2 = &hash2[key];
        if !hash1.contains_key(key) {
            let next_key = ItemKey::Key(key.as_str().unwrap().to_string());
            recurse_diffs(opts, dockey.clone(), diffs, path.push(next_key), &null_yaml, v2);
        }
    }
    if !y1.is_hash() {
        recurse_diffs(opts, dockey, diffs, path, y1, &null_yaml);
    } else if !y2.is_hash() {
        recurse_diffs(opts, dockey, diffs, path, &null_yaml,y2);
    }
}
 
fn recurse_diffs<'a>(opts: &'a Opts, dockey: Rc<DocKey>, diffs: &mut Diffs<'a>, path: KeyPath, y1: &Yaml, y2: &Yaml) {
    if y1.is_array() || y2.is_array() {
        recurse_array_diffs(opts, dockey, diffs, path, y1, y2);
    } else if y1.is_hash() || y2.is_hash() {
        recurse_hash_diffs(opts, dockey, diffs, path, y1, y2);
    } else if y1.is_null() && !y2.is_null() {
        diffs.push(Diff::add(&opts.file2,dockey,path,y2))
    } else if !y1.is_null() && y2.is_null() {
        diffs.push(Diff::remove(&opts.file1,dockey,path,y1))
    } else if *y1 != *y2 {
        diffs.push(Diff::differ(&opts.file1,&opts.file2,dockey,path,y1,y2))
    }
}

fn find_diffs<'a>(opts: &'a Opts, d1 : &Documents, d2: &Documents) -> Diffs<'a> {
    let mut diffs = Diffs::new();
    let null_yaml = Yaml::Null;
    for key in d1.keys() {
        let path = KeyPath::new();
        if d2.contains_key(key) {
            recurse_diffs(opts, Rc::new((*key).clone()), &mut diffs,path,&d1[key],&d2[key])
        } else {
            recurse_diffs(opts, Rc::new((*key).clone()), &mut diffs,path,&d1[key],&null_yaml)
        }
    }
    for key in d2.keys() {
        if !d1.contains_key(key) {
            let path = KeyPath::new();
            recurse_diffs(opts, Rc::new((*key).clone()), &mut diffs,path,&null_yaml,&d2[key])
        }
    }
    diffs
}

fn new_section<'a>(parent: &mut Option<Location<'a>>, location: &Location<'a>) -> bool {
    let new_parent = location.parent();
    if parent.is_some() && new_parent != *parent {
        *parent = new_parent;
        true
    } else {
        *parent = new_parent;
        false
    }
}

fn colorize<'a>(opts: &Opts, message: &'a str,remove: bool) -> LzyStr<'a> {
    if opts.no_colour {
        message.into()
    } else {
        let mut cmessage = ColouredStr::new(&message);
        if remove {cmessage.red()} else {cmessage.green()}
        format!("{}",cmessage).into()
    }
}

fn print_location_and_value<'a>(opts: &Opts, lav: &LocationAndValue<'a>,remove: bool) {
    let ostr = lav.value.as_str();
    let chevron = if remove {"<"} else {">"};
    if ostr.map(|s| s.contains('\n')).unwrap_or(false) {
        let text = ostr.unwrap();
        let message = format!("{} {} = ...\n{}\n",chevron,lav.loc,text);
        println!("{}",colorize(opts,&message,remove));
        if !text.ends_with("\n") { println!(); }
    } else {
        let message = format!("{} {}",chevron,lav);
        println!("{}", colorize(opts,&message,remove));
    }
}

fn show_diffs<'a>(opts: &Opts, diffs: &Diffs<'a>) -> Result<()> {
    let mut last_parent1: Option<Location<'a>> = None;
    let mut last_parent2: Option<Location<'a>> = None;
    let exclude = opts.exclude_regex()?;
    'diff: for diff in diffs {
        for re in &exclude {
            if re.is_match(&diff.key_path().to_string()) { continue 'diff; }
        }
        match diff {
            Diff::Add(lav) => {
                if new_section(&mut last_parent1, &lav.loc) { println!() }
                print_location_and_value(opts,lav,false);
            }
            Diff::Remove(lav) => {
                if new_section(&mut last_parent2, &lav.loc) { println!() }
                print_location_and_value(opts,lav,true);
            }
            Diff::Differ(lav1,lav2) => {
                let change1 = new_section(&mut last_parent1, &lav1.loc);
                let change2 = new_section(&mut last_parent1, &lav2.loc);
                if change1 || change2 { println!() }
                let str1 = lav1.value.as_str();
                let str2 = lav2.value.as_str();
                if str1.is_some() && str2.is_some() && (str1.unwrap().contains('\n') || str2.unwrap().contains('\n')) {
                    let patch = create_patch(str1.unwrap(),str2.unwrap());
                    let mut f = PatchFormatter::new();
                    if !opts.no_colour { f = f.with_color() }
                    let message = format!("< {}",lav1.loc);
                    println!("{}",colorize(opts,&message,true));
                    let message = format!("> {}",lav1.loc);
                    println!("{}",colorize(opts,&message,false));
                    print!("{}",f.fmt_patch(&patch));
                } else {
                    let message = format!("< {}",lav1);
                    println!("{}",colorize(opts,&message,true));
                    let message = format!("> {}",lav2);
                    println!("{}",colorize(opts,&message,false));
                }
            }
        }
    }
    Ok(())
}

fn do_diff(opts: &Opts) -> Result<i32> {
    let y1 = load_file(&opts.file1)?;
    let y2 = load_file(&opts.file2)?;
    let d1 = index(y1,opts).chain_err(|| format!("while indexing {}",opts.file1))?;
    let d2 = index(y2,opts).chain_err(|| format!("while indexing {}",opts.file2))?;
    let diffs = find_diffs(opts,&d1,&d2);
    show_diffs(opts,&diffs)?;
    Ok(if diffs.len() == 0 {0} else {1})
}

fn main() {
    let opts: Opts = Opts::parse();
    let result = do_diff(&opts);
    match result {
        Ok(n) => exit(n),
        Err(e) => {
            eprintln!("yamldiff: {}",e.display_chain().to_string());
            exit(2)
        }
    }
}
