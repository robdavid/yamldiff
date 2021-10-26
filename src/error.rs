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
        UnknownRenameField(field: String) {
            description("Field found in rename directive is not recognised")
            display("Unknown field '{}' not found in rename directive",field)
        }
    }
}
