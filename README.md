# YAML diff

Compare two YAML files structurally.

## Building

Build from source using the standard [Rust](https://www.rust-lang.org/tools/install) `cargo` build tool.

```bash
$ git clone https://github.com/robdavid/yamldiff.git
$ cd yamldiff
$ cargo install --path .
```
## Basic usage

Given two YAML files.

<table>
<tr>
<th> original.yaml </th> <th> modified.yaml </th>
</tr>
<tr>
<td>

```yaml
kind: ServiceAccount
metadata:
  name: vault1-agent-injector
  namespace: default
  labels:
    app.kubernetes.io/name: vault-agent-injector
    app.kubernetes.io/instance: vault1
    app.kubernetes.io/managed-by: Helm
```

</td>
<td>

```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: vault2-agent-injector
  namespace: default
  labels:
    app.kubernetes.io/name: vault-agent-injector
    app.kubernetes.io/instance: vault2
    app.kubernetes.io/managed-by: Helm
```

</td>
</td>
</tr>
</table>

These files can be compared simply with:

```text
yamldiff original.yaml modified.yaml
```

which will show the structural differences:

![singledoc](./doc/images/singledoc.png)


## Multi document files

The files to be compared can consist of multiple documents.

<table>
<tr>
<th> original-mutlidoc.yaml </th> <th> modified-multidoc.yaml </th>
</tr>
<tr>
<td>

```yaml
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: vault1-agent-injector
  namespace: default
  labels:
    app.kubernetes.io/name: vault-agent-injector
    app.kubernetes.io/instance: vault1
    app.kubernetes.io/managed-by: Helm
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: vault1
  namespace: default
  labels:
    helm.sh/chart: vault-0.17.0
    app.kubernetes.io/name: vault
    app.kubernetes.io/instance: vault1
    app.kubernetes.io/managed-by: Helm

```

</td>
<td>

```yaml
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: vault2-agent-injector
  namespace: default
  labels:
    app.kubernetes.io/name: vault-agent-injector
    app.kubernetes.io/instance: vault2
    app.kubernetes.io/managed-by: Helm
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: vault2
  namespace: default
  labels:
    helm.sh/chart: vault-0.17.0
    app.kubernetes.io/name: vault
    app.kubernetes.io/instance: vault2
    app.kubernetes.io/managed-by: Helm

```

</td>
</td>
</tr>
</table>

The documents are compared by matching them one-to-one positionally. The document index is shown in the difference output.

```text
yamldiff original-multidoc.yaml modified-multidoc.yaml
```

![singledoc](./doc/images/multidoc.png)

If any of the documents in either file cannot be matched, for example if there is an unequal number of documents between the two files, the difference is shown as deletions or insertions in the output.

```text
yamldiff original-multidoc.yaml modified.yaml
```
![singledoc](./doc/images/multi2single.png)

### Kubernetes YAML files

When comparing Kubernetes YAML files consisting of multiple documents, the documents can be matched by group, version, kind, name and namespace, rather than just position in the file, by specifying the `-k` (or `--k8s`) flag.

Consider the following two Kubernetes YAML files, which have their documents in opposite orders:

<table>
<tr>
<th> original.yaml </th> <th> modified.yaml </th>
</tr>
<tr>
<td>

```yaml
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: vault
  namespace: default
  labels:
    helm.sh/chart: vault-0.17.0
    app.kubernetes.io/name: vault
    app.kubernetes.io/instance: vault
    app.kubernetes.io/managed-by: Helm
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: vault-agent-injector
  namespace: default
  labels:
    app.kubernetes.io/name: vault-agent-injector
    app.kubernetes.io/instance: vault
    app.kubernetes.io/managed-by: Helm
```

</td>
<td>

```yaml
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: vault-agent-injector
  namespace: default
  labels:
    app.kubernetes.io/name: vault-agent-injector
    app.kubernetes.io/instance: vault
    app.kubernetes.io/managed-by: Helm
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: vault
  namespace: default
  labels:
    helm.sh/chart: vault-0.17.1
    app.kubernetes.io/name: vault
    app.kubernetes.io/instance: vault
    app.kubernetes.io/managed-by: Helm

```

</td>
</td>
</tr>
</table>

A standard diff will compare the documents in the order they appear, producing apparently several differences.

```bash
yamldiff original.yaml modified.yaml
```

![image](doc/images/naive-out-of-order.png)

However adding the `-k` flag will match documents by Kubernetes resource type, name and namespace, providing us with a more representative picture.

```bash
yamldiff -k original.yaml modified.yaml
```

![image](doc/images/sorted-out-of-order.png)

### Strategy files

Sometimes, in order to understand the differences between files, it is useful to be able to perform some transformations on the input files prior to comparison. For example, consider to the following two files.

<table>
<tr>
<th> vault1.yaml </th> <th> vault2.yaml </th>
</tr>
<tr>
<td>

```yaml
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: vault1
  namespace: default
  labels:
    helm.sh/chart: vault-0.17.0
    app.kubernetes.io/name: vault
    app.kubernetes.io/instance: vault1
    app.kubernetes.io/managed-by: Helm
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: vault1-agent-injector
  namespace: default
  labels:
    app.kubernetes.io/name: vault-agent-injector
    app.kubernetes.io/instance: vault1
    app.kubernetes.io/managed-by: Helm
```

</td>
<td>

```yaml
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: vault2-agent-injector
  namespace: default
  labels:
    app.kubernetes.io/name: vault-agent-injector
    app.kubernetes.io/instance: vault2
    app.kubernetes.io/managed-by: Helm
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: vault2
  namespace: default
  labels:
    helm.sh/chart: vault-0.17.0
    app.kubernetes.io/name: vault
    app.kubernetes.io/instance: vault2
    app.kubernetes.io/managed-by: Helm
```

</td>
</td>
</tr>
</table>

Comparing these files naively shows them to be quite different.

```bash
$ yamldiff --count vault1.yaml vault2.yaml
8 differences (additions: 1, removals: 1, changes: 6)
```
This is because the resource types don't match positionally. So lets add the `--k8s` flag to try to match by resource type.

```bash
$ yamldiff --count --k8s vault1.yaml vault2.yaml
34 differences (additions: 17, removals: 17)
```

This is even worse! Virtually every property now appears to be different. This is because the resource documents can't be matched, since they are named differently ("vault1" naming as compared with "vault2).

#### Transforming

It's possible to do some transforms on the inputs before they are matched. This allows you to work around such systemic differences to get a better picture by compensating for known differences and see what remains. The way to do this is to create a strategy file. Here is an example to deal with this case.

```yaml
transform:
  original:
    - select:
        - path: kind
          regex: .+
      replace:
        - path: metadata.name
          regex: vault1
          with: vault2
```

This is a single strategy rule that is applying a selective *transformation* on documents in the the *original* file (the first file argument). The single transform rule is *selecting* any document that has a non-empty `kind` property, and for those documents is modifying the property at the path `metadata.name`, *replacing* any occurrence of the regular expression "vault1", with "vault2". For our example, this is sufficient to transform the resource naming in the original file to match that of the modified file (in the second argument).

Assuming this is saved in a file named `strat.yaml`, you can instruct `yamldiff` to apply it's rules by specifying a `-f` (or `--strategy`) option:

```bash
yamldiff --k8s -f strat.yaml vault1.yaml vault2.yaml
```

![strategy-diff](doc/images/k8s-modified-out-of-order-with-strategy.png)

Now we are comparing like-for-like resources, and can more clearly see the differences.

The full spec of transformation rules has the following structure [^1]:

``` yaml
transform:
  original: &transform_block
    - select:
        - path: dotted.path
          value: match_value
        - path: other.dotted.path
          regex: match_expression
      replace:
        - path: dotted.path
          regex: match_expression
          with: substitution
        - path: dotted.path
          value: a_value
          with: replacement_value
      set:
        - path: dotted.path
          value: new_value
      drop: false
  modified: *transform_block
  both: *transform_block
```

* `original`  
  The rules to transform the original file (first non-option argument). Consists of a list of transform rules, all of which are applied to the file prior to comparison.
  * `select`  
    A list of rules that select YAML documents for transformation. All the criteria must match for a document to be selected.
    * `path`  
    A YAML property path, in dotted notation, of a property to be matched. If an individual property key contains a `.` character, it can be surrounded by square brackets, e.g. `"metadata.labels.[app.kubernetes.io/name]"`.
    * `regex`  
      A document is only selected if the property contains a match of this regular expression. To match against the entire property value, use regular expression `^` and `$` characters. Only a properties of type string can be matched with a regular expression.
    * `value`  
      A document is only selected if the value of the property matches this value. The type of the value can be a string, integer, float or boolean.
  * `replace`  
    A list of replacement rules that will be applied to matching documents. They will be applied in the order that they appear.
    * `path`  
    A YAML property path, in dotted notation, of a property to be modified. If an individual property key contains a `.` character, it can be surrounded by square brackets, e.g. `"metadata.labels.[app.kubernetes.io/name]"`.
    * `regex`  
      A regular expression of a substring in the property to be replaced. This is only available on string property types. All matching occurrences will be replaced.
    * `value`  
      If the property has this value, its value is modified. The type can e string, integer, float or boolean.
    * `with`  
      The replacement value. For a regular expression match, this must be a string. Capture groups can also be specified using the syntax for [Rust regex replacement strings](https://docs.rs/regex/1.1.0/regex/struct.Regex.html#replacement-string-syntax), such as `$1` for the first capture group. For value  replacement, the type can be a string, integer, float or boolean.
  * `set`  
    Unconditionally set a property to a value in selected documents.
    * `path`  
      A YAML property path, in dotted notation, of the property to be modified. If an individual property key contains a `.` character, it can be surrounded by square brackets, e.g. `"metadata.labels.[app.kubernetes.io/name]"`.
    * `value`  
      The value to be set, which can be a string, integer, float or boolean.
  * `drop`  
    If drop is true, the entire document is deleted. Incompatible with `replace` or `set`.

* `modified`  
  The rules to transform the modified file (the second non-option argument). These have the same structure as for the original file.
* `both`  
  The rules to transform both input files. These have the same structure as for the original and modified file.

### Filtering

It is also possible to perform filtering on the input files prior to indexing and comparison. There are two kinds of filtering; document filters that can selectively remove entire documents and path filters that can remove specific properties based on their paths. All filters are applied to both input files.

In the previous example, we saw the transformed file comparison yielded two differences based on a property difference found in both documents. It's possible to filter out the mismatched property in order to reduce the differences to zero. To do this, we can add path based filter rule to the strategy file:

```yaml
# New filter
filter:
  path:
    exclude:
      - name: metadata.labels.[app.kubernetes.io/instance]

# Original transform
transform:
  original:
    - select:
        - path: kind
          regex: .+
      replace:
        - path: metadata.name
          regex: vault1
          with: vault2

```

This will then prune out the non-matching property in each document, giving a final successful comparison .

```bash
$ yamldiff -ckf strat-filter.yaml vault1.yaml vault2.yaml
0 differences
```

The full specification of filter rules has the following structure[^1]:

```yaml
filter:
  document:
    exclude: &document_filter_rules
        - path: dotted.path
          value: match_value
        - path: other.dotted.path
          regex: match_expression
    include: *document_filter_rules
  path:
    exclude: &path_filter_rules
      - name: path_name
      - regex: path_pattern
    include: *path_filter_rules
```

* `filter`  
  Describes a set of filters, all of which are applied to both input files prior to comparison.
  * `document`  
    Describes a set of document filters. By default, all documents are included. If any `include` filters are given, a document must match *at least one* of these to be included. If any `exclude` filters are given, it must also not match *any* of these to be included.
    * `exclude`  
      Contains a list of property to value comparison objects. All the comparisons *must fail* for a document for it to be included.
    * `include`  
      Contains a list of comparison objects with the same structure as `exclude`. This list must either be empty, or must contain *at least one* comparison that matches for a document in order for it to be included.
      * `path`  
        The path name of the property to compare.
      * `value`  
        The comparison is true if the value of the property is equal to this value. The type of `value` can be string, integer, float or boolean.
      * `regex`  
        The comparison is true if the value of the property contains this regular expression. The type of the property must be a string.
  * `path`  
    Describes a set of path filters. Properties with paths that do not match these rules are pruned from all documents of both input files prior to indexing and comparison.
    * `exclude`  
      Contains path comparison objects. All the path comparisons *must fail* for a path for it to be included.
    * `include`  
      Contains a list of comparison objects with the same structure as `exclude`. This list must either be empty, or must contain *one or more* comparisons that match for a path in order for it to be included.
      * `name`  
        Match a path by exact name. Path names are build from keys at each level of nesting in the YAML document, joined by a `.` character. If an individual property key contains a `.` character, it will be surrounded by square brackets, e.g. `"metadata.labels.[app.kubernetes.io/name]"`
      * `regex`  
        The path comparison succeeds if the path name contains this regular expression. As with `name` the path name comprises key names joined with `.` with square brackets inserted as required. Matching any of these characters will require escaping them in the regular expression, e.g.  

        ```yaml
        regex: ^metadata\.labels\.\[app\.kubernetes\.io/name\]$
        ```

## Command Line Options

```text
USAGE:
    yamldiff [OPTIONS] <FILE1> <FILE2>

ARGS:
    <FILE1>    Original YAML file
    <FILE2>    Modified YAML file

OPTIONS:
    -c, --count                  Display the number of differences only, rather than the differences
                                 themselves
    -f, --strategy <STRATEGY>    File name of strategy file
    -h, --help                   Print help information
    -k, --k8s                    Compare kubernetes yaml documents
    -n, --no-colour              Don't produce coloured output
    -x, --exclude <EXCLUDE>      Exclude YAML document paths matching regex
```

[^1]: This example is making use of [YAML anchors (&) and references (*)](https://yaml.org/spec/1.2-old/spec.html#id2760395) to reduce repetition.
