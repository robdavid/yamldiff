# YAML diff

Compare two YAML files structurally.

## Basic usage

Given you have two YAML files.

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
$ yamldiff original.yaml modified.yaml
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
# Source: vault/templates/injector-serviceaccount.yaml
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
# Source: vault/templates/server-serviceaccount.yaml
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
# Source: vault/templates/injector-serviceaccount.yaml
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
# Source: vault/templates/server-serviceaccount.yaml
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
$ yamldiff original-multidoc.yaml modified-multidoc.yaml
```
![singledoc](./doc/images/multidoc.png)

If the number of documents in each file are unequal, the difference is shown as deletions or insertions in the output.

```text
$ yamldiff original-multidoc.yaml modified.yaml
```
![singledoc](./doc/images/multi2single.png)



