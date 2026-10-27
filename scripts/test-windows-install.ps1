# Prueba real del instalador de Windows en un runner de GitHub Actions.
# Descarga el .exe publicado, lo instala en silencio, arranca la app,
# comprueba que crea una ventana y recoge capturas y logs.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [string]$ExpectedSha256 = ""
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$outDir = Join-Path $env:RUNNER_TEMP "windows-test"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$reportPath = Join-Path $outDir "report.txt"
$eventLogPath = Join-Path $outDir "eventlog.txt"
$filesPath = Join-Path $outDir "installed-files.txt"
$summaryPath = Join-Path $outDir "summary.json"

function Write-Report {
    param([string]$Message)
    $line = "[{0}] {1}" -f (Get-Date -Format "yyyy-MM-dd HH:mm:ss"), $Message
    Write-Host $line
    Add-Content -Path $reportPath -Value $line -Encoding UTF8
}

function Write-Section {
    param([string]$Title)
    Write-Report ""
    Write-Report ("=" * 72)
    Write-Report $Title
    Write-Report ("=" * 72)
}

if (-not ("Win32" -as [type])) {
    Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;

public class Win32 {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool EnumChildWindows(IntPtr hWndParent, EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }
}
'@
}

function Get-WebView2Info {
    $versions = @()
    foreach ($p in @(
        "${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application",
        "$env:ProgramFiles\Microsoft\EdgeWebView\Application"
    )) {
        if ($p -and (Test-Path $p)) {
            $versions += Get-ChildItem -Path $p -Directory -ErrorAction SilentlyContinue |
                ForEach-Object { $_.Name }
        }
    }

    $clients = @()
    foreach ($root in @(
        "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients",
        "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients",
        "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients"
    )) {
        if (Test-Path $root) {
            foreach ($key in Get-ChildItem $root -ErrorAction SilentlyContinue) {
                $props = Get-ItemProperty $key.PSPath -ErrorAction SilentlyContinue
                if ($props -and $props.name -match "WebView2") {
                    $clients += [pscustomobject]@{
                        Key     = $key.PSChildName
                        Name    = $props.name
                        Version = $props.pv
                    }
                }
            }
        }
    }

    [pscustomobject]@{
        Versions = @($versions | Sort-Object -Unique)
        Clients  = @($clients)
    }
}

function Get-WindowTitle {
    param([IntPtr]$Handle)
    $sb = New-Object System.Text.StringBuilder 512
    [void][Win32]::GetWindowText($Handle, $sb, $sb.Capacity)
    return $sb.ToString()
}

function Find-WindowForProcess {
    param([int]$TargetProcessId)
    $script:foundWindow = [IntPtr]::Zero
    $callback = [Win32+EnumWindowsProc]{
        param([IntPtr]$hWnd, [IntPtr]$lParam)
        $windowPid = [uint32]0
        [void][Win32]::GetWindowThreadProcessId($hWnd, [ref]$windowPid)
        if ($windowPid -eq $TargetProcessId -and [Win32]::IsWindowVisible($hWnd)) {
            $sb = New-Object System.Text.StringBuilder 512
            [void][Win32]::GetWindowText($hWnd, $sb, $sb.Capacity)
            if ($sb.Length -gt 0) {
                $script:foundWindow = $hWnd
                return $false
            }
        }
        return $true
    }
    [void][Win32]::EnumWindows($callback, [IntPtr]::Zero)
    return $script:foundWindow
}

function Save-WindowScreenshot {
    param([IntPtr]$Handle, [string]$Path)
    $rect = New-Object Win32+RECT
    if (-not [Win32]::GetWindowRect($Handle, [ref]$rect)) { return $false }
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -le 0 -or $height -le 0) { return $false }

    $bmp = New-Object System.Drawing.Bitmap $width, $height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    try {
        $ok = [Win32]::PrintWindow($Handle, $hdc, 2)
    } finally {
        $g.ReleaseHdc($hdc)
        $g.Dispose()
    }
    $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    return $ok
}

Write-Section "Sistema"
try {
    $os = Get-CimInstance Win32_OperatingSystem
    Write-Report "OS: $($os.Caption) $($os.Version) build $($os.BuildNumber) $($os.OSArchitecture)"
    Write-Report "Equipo: $($env:COMPUTERNAME) Usuario: $($env:USERNAME)"
    Write-Report "PowerShell: $($PSVersionTable.PSVersion)"
} catch {
    Write-Report "No se pudo leer el sistema: $($_.Exception.Message)"
}

Write-Section "WebView2 antes de instalar"
$wv2Before = Get-WebView2Info
Write-Report "Versiones instaladas: $($wv2Before.Versions -join ', ')"
if ($wv2Before.Clients.Count -gt 0) {
    $wv2Before.Clients | Format-Table -AutoSize | Out-String -Width 240 | Add-Content $reportPath
} else {
    Write-Report "No se encontró WebView2 en el registro."
}

Write-Section "Descarga"
$setupName = "Nebula.Launcher_${Version}_x64-setup.exe"
$setupPath = Join-Path $outDir $setupName
$url = "https://github.com/DevAldx/nebula-launcher/releases/download/v$Version/$setupName"
Write-Report "URL: $url"
Invoke-WebRequest -Uri $url -OutFile $setupPath -UseBasicParsing
Unblock-File -Path $setupPath -ErrorAction SilentlyContinue
$hash = (Get-FileHash -Path $setupPath -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Report "Tamaño: $((Get-Item $setupPath).Length) bytes"
Write-Report "SHA-256: $hash"
if ($ExpectedSha256) {
    $expected = $ExpectedSha256.ToLowerInvariant()
    if ($hash -ne $expected) {
        Write-Report "ERROR: el SHA-256 no coincide. Esperado: $expected"
        exit 2
    }
    Write-Report "SHA-256 correcto."
}

Write-Section "Instalación silenciosa"
$installStart = Get-Date
$installExit = $null
try {
    $installProc = Start-Process -FilePath $setupPath -ArgumentList "/S" -Wait -PassThru
    $installExit = $installProc.ExitCode
    Write-Report "Código de salida del instalador: $installExit"
} catch {
    Write-Report "ERROR al ejecutar el instalador: $($_.Exception.Message)"
    $installExit = -1
}

Write-Section "Archivos instalados"
$programFilesX86 = [Environment]::GetFolderPath('ProgramFilesX86')
$roots = @(
    (Join-Path $env:LOCALAPPDATA "Nebula Launcher"),
    (Join-Path $env:LOCALAPPDATA "Programs\Nebula Launcher"),
    (Join-Path $env:ProgramFiles "Nebula Launcher"),
    (Join-Path $programFilesX86 "Nebula Launcher")
) | Where-Object { $_ -and (Test-Path $_) }

$exe = $null
$installDir = $null
foreach ($root in $roots) {
    Write-Report "Buscando en: $root"
    $candidate = Get-ChildItem -Path $root -Filter "*.exe" -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -notmatch '^uninstall' } |
        Sort-Object Length -Descending |
        Select-Object -First 1
    if ($candidate) {
        $exe = $candidate
        $installDir = $candidate.Directory.FullName
        break
    }
}

if (-not $exe) {
    Write-Report "No está en las rutas habituales; buscando nebula-launcher.exe en LOCALAPPDATA..."
    $candidate = Get-ChildItem -Path $env:LOCALAPPDATA -Filter "nebula-launcher.exe" -Recurse -Depth 4 -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($candidate) {
        $exe = $candidate
        $installDir = $candidate.Directory.FullName
    }
}

if ($exe) {
    Write-Report "Ejecutable: $($exe.FullName)"
    Write-Report "Tamaño: $($exe.Length) bytes"
    $vi = $exe.VersionInfo
    Write-Report "FileVersion: $($vi.FileVersion) ProductVersion: $($vi.ProductVersion)"
    Get-ChildItem -Path $installDir -Recurse -ErrorAction SilentlyContinue |
        Select-Object FullName, Length, LastWriteTime |
        Sort-Object FullName |
        Format-Table -AutoSize |
        Out-String -Width 300 |
        Set-Content -Path $filesPath -Encoding UTF8
} else {
    Write-Report "ERROR: no se encontró ningún ejecutable de Nebula."
    "No se encontraron archivos." | Set-Content -Path $filesPath -Encoding UTF8
}

$shortcutRoots = @(
    (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"),
    (Join-Path $env:PROGRAMDATA "Microsoft\Windows\Start Menu\Programs")
)
foreach ($shortcutRoot in $shortcutRoots) {
    if (Test-Path $shortcutRoot) {
        $links = Get-ChildItem -Path $shortcutRoot -Filter "*Nebula*" -Recurse -ErrorAction SilentlyContinue
        foreach ($link in $links) {
            Write-Report "Acceso directo: $($link.FullName)"
        }
    }
}

Write-Section "Arranque"
$windowHandle = [IntPtr]::Zero
$app = $null
$windowSeen = $false
if ($exe) {
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
    try {
        $app = Start-Process -FilePath $exe.FullName -WorkingDirectory $installDir -PassThru
        Write-Report "Lanzado PID $($app.Id)"
        $deadline = (Get-Date).AddSeconds(60)
        while ((Get-Date) -lt $deadline) {
            Start-Sleep -Milliseconds 500
            $app.Refresh()
            if ($app.HasExited) {
                Write-Report "La app terminó con código $($app.ExitCode)"
                break
            }
            if ($app.MainWindowHandle -ne [IntPtr]::Zero) {
                $windowHandle = $app.MainWindowHandle
                break
            }
            $found = Find-WindowForProcess -TargetProcessId $app.Id
            if ($found -ne [IntPtr]::Zero) {
                $windowHandle = $found
                break
            }
        }
        if ($windowHandle -ne [IntPtr]::Zero) {
            $windowSeen = $true
            $title = Get-WindowTitle -Handle $windowHandle
            Write-Report "Ventana encontrada: handle=$windowHandle título='$title'"
        } elseif (-not $app.HasExited) {
            Write-Report "El proceso sigue vivo pero no se encontró una ventana visible."
        }
    } catch {
        Write-Report "ERROR al lanzar la app: $($_.Exception.Message)"
    }
}

if ($app -and -not $app.HasExited) {
    Start-Sleep -Seconds 5
    $app.Refresh()
    Write-Report "Estado: vivo=$(-not $app.HasExited) WorkingSet=$([math]::Round($app.WorkingSet64 / 1MB, 1)) MB"
}

Write-Section "Procesos"
$nebulaProcs = @()
foreach ($processName in @("nebula-launcher", "Nebula Launcher")) {
    $nebulaProcs += Get-Process -Name $processName -ErrorAction SilentlyContinue
}
if ($nebulaProcs.Count -gt 0) {
    $nebulaProcs | Select-Object Id, ProcessName, MainWindowTitle, WorkingSet64 |
        Format-Table -AutoSize | Out-String -Width 240 | Add-Content $reportPath
} else {
    Write-Report "No hay procesos de Nebula."
}
$wvProcs = Get-Process -Name "msedgewebview2" -ErrorAction SilentlyContinue
if ($wvProcs) {
    Write-Report "Procesos WebView2: $($wvProcs.Count)"
    $wvProcs | Select-Object Id, ProcessName, WorkingSet64 |
        Format-Table -AutoSize | Out-String -Width 240 | Add-Content $reportPath
} else {
    Write-Report "No hay procesos msedgewebview2."
}

if ($windowHandle -ne [IntPtr]::Zero) {
    Write-Section "Ventanas hijas"
    $classes = New-Object System.Collections.Generic.List[string]
    $callback = [Win32+EnumWindowsProc]{
        param([IntPtr]$hWnd, [IntPtr]$lParam)
        $sb = New-Object System.Text.StringBuilder 256
        [void][Win32]::GetClassName($hWnd, $sb, $sb.Capacity)
        $classes.Add($sb.ToString())
        return $true
    }
    [void][Win32]::EnumChildWindows($windowHandle, $callback, [IntPtr]::Zero)
    $uniqueClasses = $classes | Sort-Object -Unique
    Write-Report "Clases: $($uniqueClasses -join ', ')"
    $uniqueClasses | Set-Content -Path (Join-Path $outDir "window-classes.txt") -Encoding UTF8
}

Write-Section "UI Automation"
try {
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($windowHandle)
    $condition = [System.Windows.Automation.Condition]::TrueCondition
    $elements = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
    $lines = @()
    foreach ($el in $elements) {
        if ($lines.Count -ge 1000) { break }
        try {
            $name = $el.Current.Name
            $type = $el.Current.ControlType.ProgrammaticName
            $aid = $el.Current.AutomationId
            if ($name -or $aid) {
                $lines += "$type`t$name`t$aid"
            }
        } catch {
            continue
        }
    }
    $lines | Set-Content -Path (Join-Path $outDir "uia-tree.txt") -Encoding UTF8
    Write-Report "Elementos UIA con nombre: $($lines.Count)"
} catch {
    Write-Report "UI Automation no disponible: $($_.Exception.Message)"
}

Write-Section "Capturas"
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
if ($windowHandle -ne [IntPtr]::Zero) {
    try {
        [void][Win32]::ShowWindow($windowHandle, 5)
        [void][Win32]::SetForegroundWindow($windowHandle)
        Start-Sleep -Seconds 2
        $windowShot = Join-Path $outDir "window.png"
        $ok = Save-WindowScreenshot -Handle $windowHandle -Path $windowShot
        Write-Report "PrintWindow: $ok -> $windowShot"
    } catch {
        Write-Report "Error en captura de ventana: $($_.Exception.Message)"
    }
}
try {
    $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
    if ($bounds.Width -gt 0 -and $bounds.Height -gt 0) {
        $bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
        $g.Dispose()
        $screenShot = Join-Path $outDir "screen.png"
        $bmp.Save($screenShot, [System.Drawing.Imaging.ImageFormat]::Png)
        $bmp.Dispose()
        Write-Report "Captura de pantalla: $screenShot ($($bounds.Width)x$($bounds.Height))"
    } else {
        Write-Report "La pantalla virtual no tiene tamaño; se omite la captura completa."
    }
} catch {
    Write-Report "Error en captura de pantalla: $($_.Exception.Message)"
}

Write-Section "WebView2/CDP"
$cdpOk = $false
try {
    $targets = Invoke-RestMethod -Uri "http://127.0.0.1:9222/json" -TimeoutSec 5
    $cdpOk = $true
    $targets | ConvertTo-Json -Depth 6 | Set-Content -Path (Join-Path $outDir "cdp-targets.json") -Encoding UTF8
    Write-Report "CDP responde. Targets: $($targets.Count)"
} catch {
    Write-Report "CDP no disponible: $($_.Exception.Message)"
}

if ($cdpOk) {
    try {
        $nodeScript = @'
const { chromium } = require('playwright-core');
(async () => {
  const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
  const pages = browser.contexts().flatMap(c => c.pages());
  console.log('pages=' + pages.length);
  for (const page of pages) {
    console.log('url=' + page.url());
    console.log('title=' + await page.title());
    const body = await page.locator('body').innerText().catch(() => '');
    console.log('body=' + body.slice(0, 6000));
    const shot = process.env.RUNNER_TEMP + '\\windows-test\\webview.png';
    await page.screenshot({ path: shot });
    console.log('screenshot=' + shot);
  }
  await browser.close();
})().catch(err => { console.error(err); process.exit(1); });
'@
        Set-Content -Path (Join-Path $outDir "cdp.cjs") -Value $nodeScript -Encoding UTF8
        Push-Location $outDir
        try {
            & npm.cmd init -y | Out-Null
            & npm.cmd install playwright-core --no-audit --no-fund --silent 2>&1 |
                Add-Content (Join-Path $outDir "npm-install.log")
            & node.exe (Join-Path $outDir "cdp.cjs") 2>&1 |
                Out-File -FilePath (Join-Path $outDir "cdp-output.txt") -Encoding UTF8
            Write-Report "Salida CDP:"
            Get-Content (Join-Path $outDir "cdp-output.txt") -ErrorAction SilentlyContinue |
                Add-Content $reportPath
        } finally {
            Pop-Location
        }
    } catch {
        Write-Report "Error usando CDP: $($_.Exception.Message)"
    }
}

Write-Section "Registro de eventos"
try {
    $events = Get-WinEvent -FilterHashtable @{ LogName = 'Application'; StartTime = $installStart } -ErrorAction SilentlyContinue |
        Where-Object { $_.Message -match 'nebula|WebView2|Application Error|Windows Error Reporting|\.NET Runtime' } |
        Select-Object TimeCreated, Id, LevelDisplayName, ProviderName, Message
    if ($events) {
        $events | Format-List | Out-String -Width 300 |
            Set-Content -Path $eventLogPath -Encoding UTF8
        Write-Report "Eventos relevantes: $($events.Count) (ver eventlog.txt)"
    } else {
        "Sin eventos relevantes." | Set-Content -Path $eventLogPath -Encoding UTF8
        Write-Report "Sin eventos relevantes."
    }
} catch {
    "No se pudo leer el registro de eventos: $($_.Exception.Message)" |
        Set-Content -Path $eventLogPath -Encoding UTF8
    Write-Report "No se pudo leer el registro de eventos: $($_.Exception.Message)"
}

$crashDirs = @(
    "$env:LOCALAPPDATA\CrashDumps",
    "$env:ProgramData\Microsoft\Windows\WER\ReportArchive"
)
$crashLines = @()
foreach ($dir in $crashDirs) {
    if (Test-Path $dir) {
        $crashLines += Get-ChildItem -Path $dir -Recurse -ErrorAction SilentlyContinue |
            Select-Object FullName, Length, LastWriteTime |
            Format-Table -AutoSize | Out-String -Width 240
    }
}
if ($crashLines.Count -gt 0) {
    $crashLines | Set-Content -Path (Join-Path $outDir "crash-dumps.txt") -Encoding UTF8
}

$wv2After = Get-WebView2Info
$windowTitle = if ($windowHandle -ne [IntPtr]::Zero) { Get-WindowTitle -Handle $windowHandle } else { $null }
$appAlive = if ($app) { -not $app.HasExited } else { $false }
$summary = [ordered]@{
    version              = $Version
    timestamp            = (Get-Date).ToString("o")
    installer_sha256     = $hash
    installer_exit       = $installExit
    install_dir          = $installDir
    installed_exe        = if ($exe) { $exe.FullName } else { $null }
    window_seen          = $windowSeen
    window_title         = $windowTitle
    app_alive            = $appAlive
    webview2_versions    = @($wv2After.Versions)
    webview2_clients     = @($wv2After.Clients)
}
$summary | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryPath -Encoding UTF8

Write-Section "Resumen"
Get-Content $summaryPath | Add-Content $reportPath

Write-Section "Desinstalación"
if ($app -and -not $app.HasExited) {
    Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
}
if ($installDir) {
    $uninstaller = Get-ChildItem -Path $installDir -Filter "uninstall*.exe" -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($uninstaller) {
        try {
            $uninstallProc = Start-Process -FilePath $uninstaller.FullName -ArgumentList "/S" -Wait -PassThru
            Write-Report "Desinstalador: $($uninstaller.FullName) exit=$($uninstallProc.ExitCode)"
        } catch {
            Write-Report "Error al desinstalar: $($_.Exception.Message)"
        }
    } else {
        Write-Report "No se encontró desinstalador."
    }
}

if ($installExit -ne 0) { exit 3 }
if (-not $exe) { exit 4 }
if (-not $windowSeen) { exit 5 }
exit 0
