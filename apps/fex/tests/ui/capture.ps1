# Capture of a window's content with PrintWindow (PW_RENDERFULLCONTENT): works even when the window
# is covered, so the user's foreground window is never touched. Crops to the visible DWM frame.
# .\capture.ps1 -ProcId <pid> -Out <file.png> [-Crop "x,y,w,h"]
param([int]$ProcId, [string]$Out, [string]$Crop = "")
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class PWC {
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, Ri, B; }
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint f);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out R r, int s);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[PWC]::SetProcessDPIAware() | Out-Null
$h = (Get-Process -Id $ProcId).MainWindowHandle
$w = New-Object PWC+R; [PWC]::GetWindowRect($h, [ref]$w) | Out-Null
$d = New-Object PWC+R; [PWC]::DwmGetWindowAttribute($h, 9, [ref]$d, 16) | Out-Null
$b = New-Object Drawing.Bitmap ($w.Ri - $w.L), ($w.B - $w.T)
$g = [Drawing.Graphics]::FromImage($b)
$hdc = $g.GetHdc()
[PWC]::PrintWindow($h, $hdc, 2) | Out-Null
$g.ReleaseHdc($hdc); $g.Dispose()
$x = $d.L - $w.L; $y = $d.T - $w.T; $cw = $d.Ri - $d.L; $ch = $d.B - $d.T
if ($Crop) { $p = $Crop.Split(','); $x += [int]$p[0]; $y += [int]$p[1]; $cw = [int]$p[2]; $ch = [int]$p[3] }
$c = $b.Clone((New-Object Drawing.Rectangle $x, $y, $cw, $ch), $b.PixelFormat)
$c.Save($Out, [Drawing.Imaging.ImageFormat]::Png)
$c.Dispose(); $b.Dispose()
"$Out ${cw}x${ch}"
