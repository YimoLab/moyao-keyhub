; 墨钥 KeyHub · NSIS 安装钩子：从旧版中文路径迁移
;
; 背景
;   0.2.x 及更早版本把 productName 定为「墨钥 KeyHub」。Tauri 的 NSIS 安装器用
;   productName 拼装安装目录与注册表键，于是默认装进
;       %LOCALAPPDATA%\墨钥 KeyHub
;   并在
;       HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\墨钥 KeyHub
;   登记卸载信息。中文路径在部分系统、子进程和命令行工具里会被错误解码，
;   因此从这里起 productName 改为英文 KeyHub，安装目录变为
;       %LOCALAPPDATA%\KeyHub
;
; 为什么需要这个钩子
;   NSIS 判断「是否已安装」依赖 Software\<manufacturer>\<productName> 注册表键。
;   产品名一改，新版安装器就找不到旧版：既不会覆盖升级，也不会清理旧文件，
;   结果是新旧两份并存（两个开始菜单项、两份程序），自动更新也会把新版装到
;   新目录而把旧目录留在原地。
;
;   所以在拷贝新文件之前主动静默卸载旧的中文版本，完成一次性迁移。
;   旧版本不存在时整段是空操作，之后的版本不受影响。

Var KeyHubLegacyUninstaller
Var KeyHubLegacyDir
Var KeyHubLegacyFirstChar

!macro NSIS_HOOK_PREINSTALL
  ; 旧版是 currentUser 安装，卸载信息在 HKCU
  ReadRegStr $KeyHubLegacyUninstaller HKCU "${WINDOWSUNINSTKEY}\墨钥 KeyHub" "UninstallString"
  ReadRegStr $KeyHubLegacyDir HKCU "${WINDOWSUNINSTKEY}\墨钥 KeyHub" "InstallLocation"

  ${If} $KeyHubLegacyUninstaller != ""
    ; /S = 静默卸载，不弹任何页面。不带 _?=，让 NSIS 把卸载器复制到临时目录后
    ; 自行删除，避免旧目录里残留 uninstall.exe。
    ExecWait '$KeyHubLegacyUninstaller /S'
  ${EndIf}

  ; InstallLocation 写入时是带引号的字符串（"C:\...\墨钥 KeyHub"），去掉首尾引号
  ${If} $KeyHubLegacyDir != ""
    StrCpy $KeyHubLegacyFirstChar $KeyHubLegacyDir 1
    ${If} $KeyHubLegacyFirstChar == "$\""
      StrCpy $KeyHubLegacyDir $KeyHubLegacyDir "" 1
      StrCpy $KeyHubLegacyDir $KeyHubLegacyDir -1
    ${EndIf}
  ${Else}
    ; 读不到记录时回退到旧版的默认安装位置
    StrCpy $KeyHubLegacyDir "$LOCALAPPDATA\墨钥 KeyHub"
  ${EndIf}

  ; 兜底：旧卸载器异常退出时也把注册表清干净，避免留下第二个卸载项
  DeleteRegKey HKCU "${WINDOWSUNINSTKEY}\墨钥 KeyHub"
  DeleteRegKey HKCU "${MANUKEY}\墨钥 KeyHub"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; 静默卸载器会把自身复制到临时目录后再执行，ExecWait 返回时旧目录可能还没删完。
  ; 此时新版文件已就位，再删一次残留。若旧版仍在运行导致删除失败，忽略即可。
  ${If} $KeyHubLegacyDir != ""
  ${AndIf} $KeyHubLegacyDir != "$INSTDIR"
    RMDir /r "$KeyHubLegacyDir"
  ${EndIf}
!macroend
