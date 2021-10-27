use serde_yaml;
use regex;

error_chain!{
    foreign_links {
        Io(std::io::Error);
        Yaml(yaml_rust::ScanError);
        SerdeYaml(serde_yaml::Error);
        Regex(regex::Error);
    }
    errors {
        KeyNotFound(key: String) {
            description("key not found in YAML document, or is wrong type")
            display("key '{}' not found in YAML document, or is wrong type",key)
        }
        WrongType(key: String) {
            description("value found in YAML document is not the correct type")
            display("value found at '{}' in YAML document is not the correct type",key)
        }
        InvalidArrayIndex(index: usize, key: String) {
            description("invalid array index in YAML document")
            display("invalid array index {} at '{}' in YAML document",index,key)
        }
        UnknownRenameField(field: String) {
            description("Field found in rename directive is not recognised")
            display("Unknown field '{}' not found in rename directive",field)
        }
    }
}
