# Chat Server (Fixed)

## Build & Run

```bash
sqlite3 chat.db < schema.sql
cargo run
```
## 記得去 https://ngrok.com/ 開 forwarding 到 localhost，需要安裝Chocolatey去install ngrok的套件指令如下 (記得用系統管理員執行powershell):

```bash
Set-ExecutionPolicy Bypass -Scope Process -Force
[System.Net.ServicePointManager]::SecurityProtocol = 3072
iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
```
## 啟動 HTTP 隧道，把本機 3000 端口映射出去
```bash
ngrok http <port>
```
###

Then check REST endpoints.
