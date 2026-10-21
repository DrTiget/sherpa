# Установщик Sherpa для Windows (PowerShell 5.1+):
#   irm https://raw.githubusercontent.com/DrTiget/sherpa/main/install.ps1 | iex
# Ставит NSIS-установщик последнего релиза тихо, для текущего пользователя (без прав администратора).
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
$ProgressPreference = 'SilentlyContinue'

$repo = 'DrTiget/sherpa'
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -Headers @{ 'User-Agent' = 'sherpa-installer' }
} catch {
    throw "Не удалось получить информацию о релизе: $($_.Exception.Message)"
}
$asset = $release.assets | Where-Object { $_.name -like '*-setup.exe' } | Select-Object -First 1
if (-not $asset) { throw "В релизе $($release.tag_name) нет установщика *-setup.exe" }

$dest = Join-Path ([IO.Path]::GetTempPath()) $asset.name
Write-Host "==> Скачиваю $($asset.name)"
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $dest -UseBasicParsing

Write-Host "==> Устанавливаю Sherpa $($release.tag_name) для текущего пользователя"
$p = Start-Process -FilePath $dest -ArgumentList '/S' -Wait -PassThru
Remove-Item $dest -ErrorAction SilentlyContinue
if ($p.ExitCode -ne 0) { throw "Установщик завершился с кодом $($p.ExitCode)" }

Write-Host "==> Готово. Запустите Sherpa из меню «Пуск»."
Write-Host "    Сборка не подписана: при ручном запуске SmartScreen может предупредить («Подробнее» -> «Выполнить в любом случае»)."
Write-Host "    Удаление: «Параметры» -> «Приложения» -> Sherpa."
