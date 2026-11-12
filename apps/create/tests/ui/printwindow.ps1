param([int]$ProcId, [string]$Out)
# Captures the main window of a process with PrintWindow: its own pixels, without bringing it to the front.
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class CreatePW {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[CreatePW]::SetProcessDPIAware() | Out-Null
$p = Get-Process -Id $ProcId
$h = $p.MainWindowHandle
if ($h -eq [IntPtr]::Zero) { Write-Output "no window"; exit 1 }
$r = New-Object CreatePW+RECT
[CreatePW]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.R - $r.L; $hh = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $hh
$g = [System.Drawing.Graphics]::FromImage($bmp)
$dc = $g.GetHdc()
[CreatePW]::PrintWindow($h, $dc, 2) | Out-Null
$g.ReleaseHdc($dc)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
Write-Output "$w x $hh"
