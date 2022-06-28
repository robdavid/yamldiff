extern crate yaml_rust;
extern crate diffy;
extern crate ansi_colors;
extern crate linked_hash_map;
extern crate regex;
extern crate serde;
extern crate serde_yaml;
extern crate clap;

pub use yaml_rust::{YamlLoader,Yaml,yaml};
use clap::Parser;
use linked_hash_map::LinkedHashMap;
use std::fmt::{Formatter,Display};
use std::rc::Rc;
use std::cmp::max;
use std::{fs,fmt};
use diffy::{create_patch,PatchFormatter};
use ansi_colors::*;
use regex::Regex;
use crate::keypath::{ItemKey,KeyPath};
use crate::error::{ErrorKind,Result,ResultExt};
use crate:: strategy::Strategy;


#[derive(Parser)]
pub struct Opts {
    file1: String,
    file2: String,
    #[clap(short,long,help="Compare kubernetes yaml documents")]
    k8s: bool,
    #[clap(short,long,help="Don't produce coloured output")]
    no_colour: bool,
    #[clap(short('x'),long,multiple_occurrences(true),help="Exclude YAML document paths matching regex")]
    exclude: Vec<String>,
    #[clap(short('f'),long,help="Difference strategy file")]
    strategy: Option<String>
}

impl Opts {
    pub fn new() -> Opts {
        Opts{file1: String::new(), file2: String::new(), k8s: false, no_colour: false, exclude: vec![], strategy: None}
    }
    fn exclude_regex(&self) -> Result<Vec<Regex>> {
        let mut result = Vec::<Regex>::new();
        for excl in &self.exclude {
            result.push(Regex::new(excl)?)
        }
        Ok(result)
    }
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

#[derive(Clone)]
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

#[derive(Clone)]
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
}

fn index(docs: Vec<Yaml>,opts: &Opts) -> Result<Documents> {
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
            let key = DocKey::K8S(K8SMeta{name,namespace,grv:GRK{api_version,kind}});
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

struct DiffContext<'a> {
    opts: &'a Opts,
    dockey: Option<Rc<DocKey>>,
    diffs: Diffs<'a>
}

fn recurse_array_diffs<'a>(ctx: &mut DiffContext<'a>, path: KeyPath, y1: &Yaml, y2: &Yaml) {
    let empty = Vec::<Yaml>::new();
    let null_yaml = Yaml::Null;
    let arr1 = y1.as_vec().unwrap_or(&empty);
    let arr2 = y2.as_vec().unwrap_or(&empty);
    let max_len = max(arr1.len(),arr2.len());
    for i in 0..max_len {
        let v1 = if i < arr1.len() { &arr1[i] } else { &null_yaml };
        let v2 = if i < arr2.len() { &arr2[i] } else { &null_yaml };
        recurse_diffs(ctx, path.push(ItemKey::Index(i)), v1, v2);
    }
    if !y1.is_array() {
        recurse_diffs(ctx, path, y1, &null_yaml);
    } else if !y2.is_array() {
        recurse_diffs(ctx, path, &null_yaml,y2);
    }
}

fn recurse_hash_diffs<'a>(ctx: &mut DiffContext<'a>, path: KeyPath, y1: &Yaml, y2: &Yaml) {
    let empty = yaml::Hash::new();
    let null_yaml = Yaml::Null;
    let hash1 = y1.as_hash().unwrap_or(&empty);
    let hash2 = y2.as_hash().unwrap_or(&empty);
    for key in hash1.keys() {
        let v1 = &hash1[key];
        let v2 = if hash2.contains_key(key) { &hash2[key] } else { &null_yaml };
        let next_key = ItemKey::Key(key.as_str().unwrap().to_string());
        recurse_diffs(ctx, path.push(next_key), v1, v2);
    }
    for key in hash2.keys() {
        let v2 = &hash2[key];
        if !hash1.contains_key(key) {
            let next_key = ItemKey::Key(key.as_str().unwrap().to_string());
            recurse_diffs(ctx, path.push(next_key), &null_yaml, v2);
        }
    }
    if !y1.is_hash() {
        recurse_diffs(ctx, path, y1, &null_yaml);
    } else if !y2.is_hash() {
        recurse_diffs(ctx, path, &null_yaml,y2);
    }
}
 
fn recurse_diffs<'a>(ctx: &mut DiffContext<'a>, path: KeyPath, y1: &Yaml, y2: &Yaml) {
    if y1.is_array() || y2.is_array() {
        recurse_array_diffs(ctx, path, y1, y2);
    } else if y1.is_hash() || y2.is_hash() {
        recurse_hash_diffs(ctx, path, y1, y2);
    } else if y1.is_null() && !y2.is_null() {
        ctx.diffs.push(Diff::add(&ctx.opts.file2,ctx.dockey.clone().unwrap(),path,y2))
    } else if !y1.is_null() && y2.is_null() {
        ctx.diffs.push(Diff::remove(&ctx.opts.file1,ctx.dockey.clone().unwrap(),path,y1))
    } else if *y1 != *y2 {
        ctx.diffs.push(Diff::differ(&ctx.opts.file1,&ctx.opts.file2,ctx.dockey.clone().unwrap(),path,y1,y2))
    }
}

fn find_diffs<'a>(opts: &'a Opts, d1 : &Documents, d2: &Documents) -> Diffs<'a> {
    let null_yaml = Yaml::Null;
    let mut ctx = DiffContext{opts,dockey: None,diffs: Diffs::new()};
    for key in d1.keys() {
        let path = KeyPath::new();
        ctx.dockey = Some(Rc::new(key.clone()));
        if d2.contains_key(key) {
            recurse_diffs(&mut ctx,path,&d1[key],&d2[key])
        } else {
            recurse_diffs(&mut ctx,path,&d1[key],&null_yaml)
        }
    }
    for key in d2.keys() {
        if !d1.contains_key(key) {
            let path = KeyPath::new();
            ctx.dockey = Some(Rc::new(key.clone()));
            recurse_diffs(&mut ctx,path,&null_yaml,&d2[key])
        }
    }
    ctx.diffs
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

fn show_diffs<'a>(opts: &Opts, diffs: &'a Diffs<'a>) -> Result<()> {
    let mut last_parent1: Option<Location<'a>> = None;
    let mut last_parent2: Option<Location<'a>> = None;
    for diff in diffs {
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

fn transform_docs(opts: &Opts, strategy: &Option<Strategy>, y1: &mut Vec<Yaml>, y2: &mut Vec<Yaml>) -> Result<()> {
    if let Some(strategy) = strategy {
        for (i,y) in y1.iter_mut().enumerate() {
            strategy.transform(y,false)
                .chain_err(|| format!("while transforming document {} of {}",i+1,opts.file1))?;
        }
        for (i,y) in y2.iter_mut().enumerate() {
            strategy.transform(y,true)
                .chain_err(|| format!("while transforming document {} of {}",i+1,opts.file2))?;
        }
    }
    Ok(())
}

fn filter_diffs<'a>(diffs: Diffs<'a>, strategy: &Option<Strategy>, exclude: &Vec<Regex>) -> Result<Diffs<'a>> {
    if strategy.is_none() && exclude.is_empty() {
        return Ok(diffs);
    }
    let mut result = vec![];
    'diff: for d in diffs {
        for re in exclude {
            if re.is_match(&d.key_path().to_string()) {
                continue 'diff;
            }
        }
        if let Some(s) = strategy {
            if !s.filter_accept(d.key_path())? {
                continue
            }
        }
        result.push(d)
    }
    Ok(result)
}

fn diff_docs<'a>(opts: &'a Opts, strategy: &'a Option<Strategy>, mut y1: Vec<Yaml>, mut y2: Vec<Yaml>) -> Result<Diffs<'a>> {
    transform_docs(opts, strategy, &mut y1, &mut y2)?;
    let d1 = index(y1,opts).chain_err(|| format!("while indexing {}",opts.file1))?;
    let d2 = index(y2,opts).chain_err(|| format!("while indexing {}",opts.file2))?;
    let exclude = opts.exclude_regex()?;
    filter_diffs(find_diffs(opts,&d1,&d2),strategy,&exclude)
}

pub fn do_diff(opts: &Opts) -> Result<i32> {
    let strategy = match &opts.strategy {
        None => None,
        Some(fname) => {
            let yaml = fs::read_to_string(fname)?;
            Some(Strategy::from_str(&yaml)?)
        }
    };
    let y1 = load_file(&opts.file1)?;
    let y2 = load_file(&opts.file2)?;
    let diffs = diff_docs(opts, &strategy, y1, y2)?;
    show_diffs(opts,&diffs)?;
    Ok(if diffs.len() == 0 {0} else {1})
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_regexfilter() {
        let test_strat = r#"
        filter:
            include:
                - pathRegex: ^metadata\.name$
        "#;
        let original = load_file("examples/vault1.yaml").unwrap();
        let modified = load_file("examples/vault2.yaml").unwrap();
        let strategy = Some(Strategy::from_str(test_strat).unwrap());
        let mut opts = Opts::new();
        opts.k8s = true;
        let diffs = diff_docs(&opts, &strategy, original, modified).unwrap();
        assert_eq!(26,diffs.len());
    }

    #[test]
    fn test_regexreplace() {
        let test_strat = r#"
        filter:
            include:
                - pathRegex: metadata\.name
        transform:
            original:
            - replace:
                - path: "metadata.name"
                  regex: "vault1"
                  with: "vault2"
        "#;
        let original = load_file("examples/vault1.yaml").unwrap();
        let modified = load_file("examples/vault2.yaml").unwrap();
        let strategy = Some(Strategy::from_str(test_strat).unwrap());
        let mut opts = Opts::new();
        opts.k8s = true;
        let diffs = diff_docs(&opts, &strategy, original, modified).unwrap();
        assert_eq!(0,diffs.len());
    }
}