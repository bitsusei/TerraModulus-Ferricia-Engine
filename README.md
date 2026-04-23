# TerraModulus Ferricia Engine

> [!NOTE]
> This repository is standardized by [EFP 5](https://anvilloydevstudio.github.io/TerraModulus-EFPs/efp/efp005).

## Open Dynamics Engine (ODE)

Note that there exists a binding library of ODE, [`ode-base`](https://crates.io/crates/ode-base),
but it is not actively maintained, which is not ideal for continuous development of this
project. Instead, a custom binding crate has been made in [`ode`](/ode), which is not published.

## Building

Running cargo profiles requires respective native libraries of ODE, OpenAL, SDL3.
Therefore, running on Windows requires specifying `target/debug` (which includes the libraries)
in `PATH` as an environment variable.

For example, in PowerShell on Windows, this may be executed beforehand:
```ps1
$env:PATH = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User") + ";" + $(Convert-Path "target\debug")
```
