# Requires -PSEdition Core -RunAsAdministrator

$env:PATH = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User") + ";" + $(Convert-Path "target\debug")

cargo flamegraph --example physics_collision --dev
