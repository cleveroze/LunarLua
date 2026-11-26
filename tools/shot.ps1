param(
  [string]$Title = "LunarLua",
  [string]$Out = "build\shot.png",
  [int]$Wait = 2500
)

Add-Type -AssemblyName System.Drawing

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class LunarWin {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string name);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
}
"@

$h = [IntPtr]::Zero
for ($i = 0; $i -lt 40; $i++) {
  $h = [LunarWin]::FindWindow($null, $Title)
  if ($h -ne [IntPtr]::Zero) { break }
  Start-Sleep -Milliseconds 250
}
if ($h -eq [IntPtr]::Zero) { Write-Error "window '$Title' not found"; exit 1 }

[LunarWin]::ShowWindow($h, 5) | Out-Null
[LunarWin]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds $Wait

$r = New-Object LunarWin+RECT
[LunarWin]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.Right - $r.Left
$ht = $r.Bottom - $r.Top
if ($w -le 0 -or $ht -le 0) { Write-Error "bad window rect"; exit 1 }

$bmp = New-Object System.Drawing.Bitmap $w, $ht
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
# 2 = PW_RENDERFULLCONTENT (works with DWM composition)
[LunarWin]::PrintWindow($h, $hdc, 2) | Out-Null
$g.ReleaseHdc($hdc)
$g.Dispose()

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
$bmp.Save((Resolve-Path -LiteralPath (Split-Path -Parent $Out)).Path + "\" + (Split-Path -Leaf $Out), [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "saved $Out ($w x $ht)"
