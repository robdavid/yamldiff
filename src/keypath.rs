use std::fmt::{Formatter,Display};
use std::fmt;
use yaml_rust::Yaml;
use yaml_rust::yaml;
use crate::error::{Result,ErrorKind};

/**
 * Component of a path in the document heirarchy. Either an array index
 * or a hash key.
 */
#[derive(PartialEq,Clone,Debug)]
pub enum ItemKey {
    Index(usize),
    Key(String)
}

impl From<&str> for ItemKey {
    fn from(name: &str) -> ItemKey {
        ItemKey::Key(name.to_string())
    }
}

impl From<usize> for ItemKey {
    fn from(index: usize) -> ItemKey {
        ItemKey::Index(index)
    }
}

/**
 * A path in the document heirarchy as a vector of path components.
 */
 #[derive(PartialEq,Clone,Debug)]
 pub struct KeyPath(pub Vec<ItemKey>);
 
 struct ParseContext<'a> {
    pathstr: &'a str,
    path: KeyPath,
    item: String,
    bnest: i16,
    bstart: usize,
    bpos: usize,
    bstack: Vec<Brace<'a>>
 }

 struct Brace<'a> {
    bstart: usize,
    bstr: &'a str,
 }

 impl<'a> ParseContext<'a> {
    fn new(pathstr: &'a str) -> ParseContext<'a> {
            ParseContext{pathstr, 
            path:   KeyPath::new(), 
            item:   String::new(),
            bnest:  0,
            bpos:   0,
            bstart: 0,
            bstack: Vec::new(),
        }
    }
    fn push_brace(&mut self, endpos: usize) {
        self.bstack.push(Brace{bstart: self.bstart, bstr: &self.pathstr[self.bpos..endpos]})
    }
    fn pop_brace(&mut self) -> Option<Brace<'a>> {
        self.bstack.pop()
    }
    fn bra(&mut self, i: usize) {
        if self.bnest == 0 {
            self.bpos = i+1;
            self.bstart = self.item.len();
        }
        self.bnest +=1;
    }
    fn ket(&mut self, i: usize) {
        if self.bnest > 0 {
            self.bnest-=1;
            if self.bnest == 0 {
                self.push_brace(i);
            }
        }
    }

    fn push(&mut self, ch: char) {
        self.item.push(ch);
    }



    fn complete_item(&mut self) {
        if !self.item.is_empty() {
            let start = self.path.0.len();
            loop {
                match self.pop_brace() {
                    Some(brace) => {
                        if brace.bstart + brace.bstr.len() == self.item.len() {
                            let rindex = brace.bstr.parse::<usize>();
                            if let Ok(index) = rindex {
                                // Array index syntax
                                self.path.0.push(ItemKey::from(index));
                                self.item.truncate(self.item.len() - brace.bstr.len())
                            }
                        }
                    },
                    None => break
                }
            }
            if !self.item.is_empty() {
                let mut item = String::new();
                std::mem::swap(&mut item,&mut self.item);
                self.path.0.push(ItemKey::Key(item));
            }
            self.path.0[start..].reverse();
        }
    }
 }

 impl KeyPath {
     pub fn new() -> KeyPath {
         KeyPath(Vec::<ItemKey>::new())
     }
     pub fn push(&self,key: ItemKey) -> KeyPath {
         let mut newvec = self.0.clone();
         newvec.push(key);
         KeyPath(newvec)
     }
     #[allow(unused)]
     pub fn truncated(&self,len: usize) -> KeyPath {
         let mut newvec = self.0.clone();
         newvec.truncate(len);
         KeyPath(newvec)
     }
     pub fn parse(pathstr: &str) -> KeyPath {
        let mut ctx = ParseContext::new(pathstr);
        for (i,ch) in pathstr.chars().enumerate() {
            match ch {
                '[' => ctx.bra(i),
                ']' => ctx.ket(i),
                '.' if ctx.bnest == 0 => ctx.complete_item(),
                _ => ctx.push(ch)
            }
        }
        ctx.complete_item();
        ctx.path
     }
 }

 impl From<&[ItemKey]> for KeyPath {
     fn from(items: &[ItemKey]) -> KeyPath {
         KeyPath(Vec::<ItemKey>::from(items))
     }
 }

 impl From<&str> for KeyPath {
     fn from(path: &str) -> KeyPath {
         KeyPath::parse(path)
     }
 }
 
 impl From<&[&str]> for KeyPath {
    fn from(path: &[&str]) -> KeyPath {
         let keys = path.iter().map(|key| ItemKey::from(*key)).collect();
         KeyPath(keys)
    }
 }

 impl Display for KeyPath {
     fn fmt(&self, f: &mut Formatter) -> fmt::Result {
         let mut first = true;
         for item in &self.0 {
             match item {
                 ItemKey::Index(u) => { write!(f,"[{}]",u)?; }
                 ItemKey::Key(str) => {
                     let sep = if first {""} else {"."};
                     if str.contains(".") {
                         write!(f,"{}[{}]",sep,str)?;
                     } else {
                         write!(f,"{}{}",sep,str)?;
                     }
                 }
             }
             first = false
         }
         Ok(())
     }
 }

 pub trait KeyPathFuncs: std::marker::Sized {
    fn set_at_path<T: Into<KeyPath>>(&mut self,path: T, value: Self) -> Result<()>;
    fn get_at_path<T: Into<KeyPath>>(&self,path: T) -> Result<&Self>;
 }

 impl KeyPathFuncs for Yaml {
    fn set_at_path<T: Into<KeyPath>>(&mut self, path: T, value: Yaml) -> Result<()> {
        let mut current: &mut Yaml = self;
        let path = path.into();
        let mut iter = path.0.into_iter().peekable();
        let mut processing = KeyPath::new();
        while let Some(item) = iter.next() {
            let item_copy = item.clone();
            match item {
                ItemKey::Key(key) => {
                    if let Yaml::Hash(h) = current {
                        match iter.peek() {
                            None => { 
                                h.insert(Yaml::String(key),value);
                                break; 
                            }
                            Some(next_item) => {
                                let ykey = Yaml::String(key);
                                if !h.contains_key(&ykey) {
                                    match next_item {
                                        ItemKey::Key(_) => { h.insert(ykey.clone(),Yaml::Hash(yaml::Hash::new())); }
                                        ItemKey::Index(_) => { h.insert(ykey.clone(),Yaml::Array(yaml::Array::new())); }
                                    }
                                }
                                current = &mut h[&ykey];
                            }
                        }
                    } else {
                        return Err(ErrorKind::WrongType(processing.to_string()).into());
                    }
                }
                ItemKey::Index(index) => {
                    if let Yaml::Array(a) = current { 
                        if index >= a.len() {
                            return Err(ErrorKind::InvalidArrayIndex(index,processing.to_string()).into());
                        }
                        match iter.peek() {
                            None => { 
                                a[index] = value;
                                break; 
                            }
                            Some(_) => {
                                current = &mut a[index]
                            }
                        }
                    } else { 
                        return Err(ErrorKind::WrongType(processing.to_string()).into());
                    }
                }
            }
            processing.0.push(item_copy);
        }
        Ok(())
    }

    fn get_at_path<T: Into<KeyPath>>(&self, path: T) -> Result<&Yaml> {
        let path = path.into();
        let mut processed_path = KeyPath::new();
        let mut result = self;
        for item in path.0 {
            processed_path.0.push(item.clone());
            match item {
                ItemKey::Key(key) => { result = &result[key.as_str()]; }
                ItemKey::Index(index) => { result = &result[index]; }
            }
            if result.is_badvalue() {
                return Err(ErrorKind::KeyNotFound(processed_path.to_string()).into())
            }
        }
        Ok(result)
    }
 }
 
 #[cfg(test)]
 mod test {
     use super::*;
     use crate::yamldiff::yaml::YamlLoader;

     #[test]
     fn test_parse_string_keys() {
         let kp = KeyPath::parse("a.b.c.d");
         let expected: &[ItemKey] = &[ItemKey::from("a"),ItemKey::from("b"),ItemKey::from("c"),ItemKey::from("d")];
         assert_eq!(KeyPath::from(expected),kp);
     }

     #[test]
     fn test_parse_string_keys_quoted() {
        let kp = KeyPath::parse("a.b.[c.d]");
        let expected: &[ItemKey] = &[ItemKey::from("a"),ItemKey::from("b"),ItemKey::from("c.d")];
        assert_eq!(KeyPath::from(expected),kp);
    }

    #[test]
    fn test_parse_string_and_index_keys() {
        let kp = KeyPath::parse("a.b.c[0]");
        let expected: &[ItemKey] = &[ItemKey::from("a"),ItemKey::from("b"),ItemKey::from("c"),ItemKey::from(0)];
        assert_eq!(KeyPath::from(expected),kp);
    }

    #[test]
    fn test_parse_string_and_2d_index_keys() {
        let kp = KeyPath::parse("a.b.c[0][1]");
        let expected: &[ItemKey] = &[ItemKey::from("a"),ItemKey::from("b"),ItemKey::from("c"),ItemKey::from(0),ItemKey::from(1)];
        assert_eq!(KeyPath::from(expected),kp);
    }

    #[test]
    fn test_parse_string_and_quoted_with_index_key() {
        let kp = KeyPath::parse("a.[b.c][1]");
        let expected: &[ItemKey] = &[ItemKey::from("a"),ItemKey::from("b.c"),ItemKey::from(1)];
        assert_eq!(KeyPath::from(expected),kp);
    }

    #[test]
    fn test_parse_string_and_numeric_quoted_with_index_key() {
        let kp = KeyPath::parse("a.b.c[0]d[1]");
        let expected: &[ItemKey] = &[ItemKey::from("a"),ItemKey::from("b"),ItemKey::from("c0d"),ItemKey::from(1)];
        assert_eq!(KeyPath::from(expected),kp);
    }

    #[test]
    fn test_parse_long_string_and_numeric_quoted_with_index_key() {
        let kp = KeyPath::parse("apple.banana.coconut[0]date[1]");
        let expected: &[ItemKey] = &[ItemKey::from("apple"),ItemKey::from("banana"),ItemKey::from("coconut0date"),ItemKey::from(1)];
        assert_eq!(KeyPath::from(expected),kp);
    }

    #[test]
    fn test_set_value() {
        let yaml = r#"
        this:
            is:
                - a:
                    deep:
                        path: 123
        "#;
        let mut y = YamlLoader::load_from_str(yaml).unwrap();
        assert_eq!(y[0]["this"]["is"][0]["a"]["deep"]["path"],Yaml::Integer(123));
        y[0].set_at_path("this.is[0].a.deep.path",Yaml::Integer(456)).unwrap();
        assert_eq!(y[0]["this"]["is"][0]["a"]["deep"]["path"],Yaml::Integer(456));
    }

    #[test]
    fn test_set_new_value() {
        let mut y = Yaml::Hash(yaml::Hash::new());
        y.set_at_path("this.is.a.deep.path",Yaml::Integer(456)).unwrap();
        assert_eq!(y["this"]["is"]["a"]["deep"]["path"],Yaml::Integer(456));
    }

    #[test]
    fn test_set_new_bad_array_value() {
        let mut y = Yaml::Hash(yaml::Hash::new());
        let result = y.set_at_path("this_is_array[0]",Yaml::Integer(456));
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!("invalid array index 0 at 'this_is_array' in YAML document",e.to_string());
        }
    }

    #[test]
    fn test_set_new_bad_path_array_value() {
        let mut y = Yaml::Hash(yaml::Hash::new());
        let result = y.set_at_path("this.is.array[0].obj",Yaml::Integer(456));
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!("invalid array index 0 at 'this.is.array' in YAML document",e.to_string());
        }
    }

    #[test]
    fn test_set_bad_path_value() {
        let yaml = r#"
        this:
            is:
                - a:
                    deep:
                        path: 123
        "#;
        let mut y = YamlLoader::load_from_str(yaml).unwrap();
        assert_eq!(y[0]["this"]["is"][0]["a"]["deep"]["path"],Yaml::Integer(123));
        let result = y[0].set_at_path("this.is[0].a.deep.path.bad",Yaml::Integer(456));
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!("value found at 'this.is[0].a.deep.path' in YAML document is not the correct type",e.to_string());
        }
    }

    #[test]
    fn test_set_bad_mid_path_value() {
        let yaml = r#"
        this:
            is:
                - a:
                    deep: 123
        "#;
        let mut y = YamlLoader::load_from_str(yaml).unwrap();
        assert_eq!(y[0]["this"]["is"][0]["a"]["deep"],Yaml::Integer(123));
        let result = y[0].set_at_path("this.is[0].a.deep.really.bad",Yaml::Integer(456));
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!("value found at 'this.is[0].a.deep' in YAML document is not the correct type",e.to_string());
        }
    }

    #[test]
    fn test_get_path_value() {
        let yaml = r#"
        this:
            is:
                - a:
                    deep:
                        path: 123
        "#;
        let y = YamlLoader::load_from_str(yaml).unwrap();
        let result = y[0].get_at_path("this.is[0].a.deep.path").unwrap();
        assert_eq!(Yaml::Integer(123),*result);
    }

    #[test]
    fn test_get_bad_path_value() {
        let yaml = r#"
        this:
            is:
                - a:
                    deep:
                        path: 123
        "#;
        let y = YamlLoader::load_from_str(yaml).unwrap();
        let result = y[0].get_at_path("this.is[0].a.steep.path");
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!("key 'this.is[0].a.steep' not found in YAML document, or is wrong type",e.to_string());
        }
    }

}