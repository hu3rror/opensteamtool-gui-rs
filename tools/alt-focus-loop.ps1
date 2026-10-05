# 单实例焦点回归脚本（ADR-0017 验收手段）：低层键盘钩子断言「重复启动不再注入击键」。
# 背景：winit 的 force_window_active 用 SendInput 模拟 Alt 绕过 Windows 前台锁，注入的
# Alt 会命中当时前台窗口的菜单栏/命令栏首项（资源管理器「新建」被选中）。修复见 ADR-0017。
# 判定：日志三处 count 均为 0（t+150/400/800ms 无注入击键）即绿；出现 LALTDN/LALTUP 即红。
# 用法：pwsh -File tools/alt-focus-loop.ps1 [-Exe <exe>] [-ExplorerPath <目录>]（需 pwsh 7+、交互桌面会话）
#   默认 Exe=仓库 target\debug 构建；验证前确认 exe 与当前源码同步（exe 修改时间新于 src）。
# 副作用：会强制结束正在运行的 opensteamtool-manager 实例，并短暂打开一个资源管理器窗口
#   作为前台受害者——应用正在使用时不要跑。
param(
    [string]$Exe = "$PSScriptRoot\..\target\debug\opensteamtool-manager.exe",
    [string]$ExplorerPath = "$HOME\Downloads"
)
$ErrorActionPreference = 'Continue'
function Log($m) { Write-Host ("[{0}] {1}" -f (Get-Date -Format HH:mm:ss.fff), $m) }
Add-Type -TypeDefinition @'
using System; using System.Collections.Concurrent; using System.Runtime.InteropServices; using System.Threading;
public static class P {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool keybd_event(byte vk, byte sc, uint f, UIntPtr e);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr GetModuleHandle(string n);
  private delegate IntPtr LL(int n, IntPtr w, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] private struct K { public uint vk; public uint sc; public uint f; public uint t; public IntPtr e; }
  [StructLayout(LayoutKind.Sequential)] private struct M { public IntPtr h; public uint m; public IntPtr w; public IntPtr l; public uint t; public int x; public int y; }
  [DllImport("user32.dll")] private static extern IntPtr SetWindowsHookEx(int id, LL cb, IntPtr h, uint t);
  [DllImport("user32.dll")] private static extern IntPtr CallNextHookEx(IntPtr h, int n, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] private static extern bool GetMessageW(out M m, IntPtr h, uint mn, uint mx);
  [DllImport("user32.dll")] private static extern bool TranslateMessage(ref M m);
  [DllImport("user32.dll")] private static extern IntPtr DispatchMessageW(ref M m);
  private static IntPtr _hook; private static readonly ConcurrentQueue<string> _q = new ConcurrentQueue<string>();
  private static IntPtr CB(int n, IntPtr w, IntPtr l) { if (n>=0) { var k=(K)Marshal.PtrToStructure(l, typeof(K)); bool inj=(k.f&0x10)!=0; string ky = k.vk==0xA4?"LALT":"VK_"+k.vk.ToString(); string d=(k.f&0x80)!=0?"UP":"DN"; _q.Enqueue(ky+d+(inj?"+INJ":"")); } return CallNextHookEx(_hook,n,w,l); }
  public static void Start() { Thread t = new Thread(()=>{ _hook=SetWindowsHookEx(13,new LL(CB),GetModuleHandle(null),0); M m; while(GetMessageW(out m,IntPtr.Zero,0,0)){TranslateMessage(ref m);DispatchMessageW(ref m);} }); t.IsBackground=true; t.SetApartmentState(ApartmentState.STA); t.Start(); }
  public static string[] Drain() { var r = new System.Collections.Generic.List<string>(); string e; while(_q.TryDequeue(out e)) r.Add(e); return r.ToArray(); }
  public static void TapAlt() { keybd_event(0xA4, 0x38, 1, UIntPtr.Zero); keybd_event(0xA4, 0x38, 3, UIntPtr.Zero); }
  public static void Act(IntPtr h) { ShowWindow(h,9); SetWindowPos(h,new IntPtr(0),0,0,0,0,0x0003); TapAlt(); SetForegroundWindow(h); }
}
'@
[P]::Start(); Start-Sleep -Milliseconds 200
Get-Process opensteamtool-manager -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 400
$app = Start-Process -FilePath $Exe -PassThru
for ($i=0; $i -lt 60 -and $app.MainWindowHandle -eq 0; $i++) { Start-Sleep -Milliseconds 100; $app.Refresh() }
$appHwnd = $app.MainWindowHandle
Log "app hwnd=$appHwnd"
$known = @(Get-Process explorer -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | ForEach-Object { $_.Id })
Start-Process -FilePath explorer.exe -ArgumentList "/n,$ExplorerPath" | Out-Null
$vict = $null
for ($i=0; $i -lt 40 -and -not $vict; $i++) { Start-Sleep -Milliseconds 150; $vict = Get-Process explorer | Where-Object { $_.MainWindowHandle -ne 0 -and $known -notcontains $_.Id } | Select-Object -First 1 }
$vh = $vict.MainWindowHandle; Log "victim hwnd=$vh"
$ok = $false
for ($i=0; $i -lt 5 -and -not $ok; $i++) { [P]::Act($vh); Start-Sleep -Milliseconds 250; $ok = ([P]::GetForegroundWindow() -eq $vh) }
Log "victim fg=$ok ; app visible=$([P]::IsWindowVisible($appHwnd)) iconic=$([P]::IsIconic($appHwnd))"
$null = [P]::Drain()
if ($ok) {
    $p2 = Start-Process -FilePath $Exe -PassThru
    Log "spawned dup pid=$($p2.Id)"
    Start-Sleep -Milliseconds 150; $e1=[P]::Drain(); Log ("t+150ms count=" + $e1.Count + " -> " + ($e1 -join ','))
    Start-Sleep -Milliseconds 250; $e2=[P]::Drain(); Log ("t+400ms count=" + $e2.Count + " -> " + ($e2 -join ','))
    Start-Sleep -Milliseconds 400; $e3=[P]::Drain(); Log ("t+800ms count=" + $e3.Count + " -> " + ($e3 -join ','))
    $p2.WaitForExit(1500)|Out-Null
    Log "post fg=$([P]::GetForegroundWindow()) app=$appHwnd"
} else { Log "SKIP" }
[P]::SendMessage($vh, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Get-Process opensteamtool-manager -ErrorAction SilentlyContinue | Stop-Process -Force
Log "done"
