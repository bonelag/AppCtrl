import type { Component } from 'solid-js';
import { Show, For, createSignal, onCleanup, createEffect } from 'solid-js';
import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { AppProvider, useApp } from './store/appStore';
import type { AppType, AppConfig } from './types';
import './index.css';


const App: Component = () => {
  return (
    <AppProvider>
      <MainWindow />
    </AppProvider>
  );
};

// Main Window
const MainWindow: Component = () => {
  const [store, actions] = useApp();
  const [selectedAppId, setSelectedAppId] = createSignal<string | null>(null);

  // Check status of all apps on load
  const checkAllAppsStatus = async () => {
    for (const app of store.apps) {
      if (app.appType === 'exe' || app.executablePath.toLowerCase().endsWith('.exe')) {
        try {
          const isRunning = await invoke<boolean>('check_process_running', { exePath: app.executablePath });
          if (isRunning !== app.isRunning) {
            actions.setAppRunning(app.id, isRunning);
          }
        } catch (e) {
          console.error(`Failed to check status for ${app.name}:`, e);
        }
      }
    }
  };

  // Listen for app-stopped event
  let unlistenStopped: UnlistenFn | undefined;
  const setupStoppedListener = async () => {
    unlistenStopped = await listen<{ appId: string }>('app-stopped', (event) => {
      actions.setAppRunning(event.payload.appId, false);
    });

    // Initial check
    checkAllAppsStatus();

    // Periodically check (every 5s)
    const interval = setInterval(checkAllAppsStatus, 5000);
    onCleanup(() => clearInterval(interval));
  };
  setupStoppedListener();
  onCleanup(() => { if (unlistenStopped) unlistenStopped(); });

  // Sync tray setting with backend whenever it changes
  createEffect(() => {
    const minimize = store.settings.minimizeToTray;
    invoke('set_minimize_to_tray', { minimize }).catch(console.error);
  });

  const toggleTheme = () => {
    const newTheme = store.settings.theme === 'dark' ? 'light' : 'dark';
    actions.updateSettings({ theme: newTheme });
  };

  return (
    <div class={`h-screen w-screen flex flex-col overflow-hidden transition-colors duration-300
      ${store.settings.theme === 'dark'
        ? 'bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 text-white'
        : 'bg-gradient-to-br from-gray-50 via-gray-100 to-gray-200 text-gray-900'
      }`}>
      {/* Header */}
      <header class={`h-14 flex items-center justify-between px-5 backdrop-blur-sm border-b flex-shrink-0 transition-colors duration-300
        ${store.settings.theme === 'dark'
          ? 'bg-black/30 border-white/10'
          : 'bg-white/50 border-black/5'
        }`}>
        <div class="flex items-center gap-3">
          <div class="w-8 h-8 rounded-lg bg-gradient-to-br from-blue-500 to-purple-600 flex items-center justify-center shadow-lg">
            <span class="text-sm font-bold text-white">⚡</span>
          </div>
          <span class="font-semibold text-lg tracking-tight">AppCtrl</span>
        </div>
        <div class="flex items-center gap-2">
          <button
            onClick={toggleTheme}
            class={`w-9 h-9 rounded-lg flex items-center justify-center transition-all hover:scale-105
              ${store.settings.theme === 'dark' ? 'bg-white/5 hover:bg-white/10' : 'bg-black/5 hover:bg-black/10'}`}
            title={store.settings.theme === 'dark' ? 'Switch to Light Mode' : 'Switch to Dark Mode'}
          >
            {store.settings.theme === 'dark' ? '🌙' : '☀️'}
          </button>
          <button
            onClick={actions.openSettingsModal}
            class={`w-9 h-9 rounded-lg flex items-center justify-center transition-all hover:scale-105
              ${store.settings.theme === 'dark' ? 'bg-white/5 hover:bg-white/10' : 'bg-black/5 hover:bg-black/10'}`}
            title="Settings"
          >
            ⚙️
          </button>
          <button
            onClick={actions.openAddModal}
            class="h-9 px-4 rounded-lg bg-gradient-to-r from-blue-500 to-purple-600 hover:from-blue-600 hover:to-purple-700 flex items-center gap-2 text-sm font-medium transition-all hover:scale-105 shadow-lg shadow-blue-500/25 text-white"
          >
            <span class="text-lg">+</span>
            <span>Add</span>
          </button>
        </div>
      </header>

      {/* App Grid */}
      <main class="flex-1 p-4 overflow-auto">
        <Show
          when={store.apps.length > 0}
          fallback={
            <div class="flex items-center justify-center h-full">
              <div class={`text-center p-6 rounded-2xl border ${store.settings.theme === 'dark' ? 'bg-white/5 border-white/10' : 'bg-black/5 border-black/5'}`}>
                <div class="text-4xl mb-3">📦</div>
                <p class={store.settings.theme === 'dark' ? 'text-white/60' : 'text-black/60'}>Chưa có ứng dụng</p>
                <p class={`text-sm mt-1 ${store.settings.theme === 'dark' ? 'text-white/40' : 'text-black/40'}`}>Nhấn + để thêm</p>
              </div>
            </div>
          }
        >
          <div class="grid grid-cols-3 sm:grid-cols-4 gap-3">
            <For each={store.apps}>
              {(app) => (
                <AppCard
                  app={app}
                  isSelected={selectedAppId() === app.id}
                  onSelect={() => setSelectedAppId(app.id)}
                />
              )}
            </For>
          </div>
        </Show>
      </main>

      {/* Log Panel */}
      <Show when={selectedAppId()}>
        <LogPanel appId={selectedAppId()!} onClose={() => setSelectedAppId(null)} />
      </Show>

      {/* Modals */}
      <Show when={store.modal.type === 'add' || store.modal.type === 'edit'}>
        <AppModal />
      </Show>
      <Show when={store.modal.type === 'settings'}>
        <SettingsModal />
      </Show>
    </div>
  );
};

// App Card
const AppCard: Component<{
  app: AppConfig;
  isSelected: boolean;
  onSelect: () => void;
}> = (props) => {
  const [store, actions] = useApp();
  const [isHovered, setIsHovered] = createSignal(false);

  const handleRun = async (e: Event) => {
    e.stopPropagation();
    if (props.app.isRunning) {
      await invoke('stop_app', {
        appId: props.app.id,
        exePath: props.app.executablePath
      });
      actions.setAppRunning(props.app.id, false);
    } else {
      actions.clearLogs(props.app.id);
      actions.setAppRunning(props.app.id, true);
      props.onSelect();

      try {
        await invoke('start_app', {
          appId: props.app.id,
          path: props.app.executablePath,
          appType: props.app.appType,
          workingDir: props.app.workingDirectory || '',
          args: props.app.arguments || '',
          envVars: props.app.environmentVars || '',
        });
      } catch (err) {
        actions.appendLog(props.app.id, `❌ Error: ${err}`);
        actions.setAppRunning(props.app.id, false);
      }
    }
  };

  const handleEdit = (e: Event) => {
    e.stopPropagation();
    actions.openEditModal(props.app.id);
  };

  const handleDelete = async (e: Event) => {
    e.stopPropagation();
    const { ask } = await import('@tauri-apps/plugin-dialog');
    const confirmed = await ask(`Xóa "${props.app.name}"?`, {
      title: 'Xác nhận xóa',
      kind: 'warning',
    });
    if (confirmed) {
      actions.deleteApp(props.app.id);
    }
  };

  const getTypeIcon = () => {
    switch (props.app.appType) {
      case 'exe': return '🖥️';
      case 'bat': return '📄';
      case 'shell': return '💻';
      default: return '📦';
    }
  };

  return (
    <div
      class={`relative group p-3 rounded-xl border 
          transition-all duration-200 cursor-pointer hover:scale-105
          ${store.settings.theme === 'dark'
          ? 'bg-white/5 border-white/10 hover:bg-white/10'
          : 'bg-white border-black/5 hover:bg-gray-50 shadow-sm'}
          ${props.isSelected ? 'ring-2 ring-blue-500/50' : ''}
          ${props.app.isRunning ? 'ring-2 ring-green-500/50' : ''}`}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      onClick={props.onSelect}
    >
      {/* Icon */}
      <div class={`w-12 h-12 mx-auto mb-2 rounded-lg flex items-center justify-center overflow-hidden
        ${store.settings.theme === 'dark' ? 'bg-gradient-to-br from-white/10 to-white/5' : 'bg-gray-100'}`}>
        <Show
          when={props.app.icon}
          fallback={<span class="text-xl">{getTypeIcon()}</span>}
        >
          <img src={props.app.icon!} alt={props.app.name} class="w-full h-full object-cover image-crisp" />
        </Show>
      </div>

      {/* Name */}
      <p class={`text-center text-xs font-medium truncate ${store.settings.theme === 'dark' ? 'text-white' : 'text-gray-800'}`}>{props.app.name}</p>

      {/* Running indicator */}
      <Show when={props.app.isRunning}>
        <div class="absolute top-1.5 right-1.5 w-2 h-2 rounded-full bg-green-400 animate-pulse" />
      </Show>

      {/* Hover Controls */}
      <Show when={isHovered()}>
        <div class="absolute inset-0 bg-black/70 backdrop-blur-sm rounded-xl flex items-center justify-center">
          {/* Play/Stop */}
          <button
            onClick={handleRun}
            class={`w-10 h-10 rounded-full flex items-center justify-center text-lg transition-transform hover:scale-110
                ${props.app.isRunning ? 'bg-red-500' : 'bg-green-500'}`}
          >
            {props.app.isRunning ? '⏹' : '▶'}
          </button>

          {/* Edit */}
          <button
            onClick={handleEdit}
            class="absolute top-1.5 left-1.5 w-6 h-6 rounded bg-white/10 hover:bg-white/20 flex items-center justify-center text-xs"
          >
            ✏️
          </button>

          {/* Delete */}
          <button
            onClick={handleDelete}
            class="absolute top-1.5 right-1.5 w-6 h-6 rounded bg-red-500/30 hover:bg-red-500/50 flex items-center justify-center text-xs"
          >
            🗑️
          </button>
        </div>
      </Show>
    </div>
  );
};

// Parse text and make URLs clickable
function parseLogLine(text: string): { type: 'text' | 'link'; content: string }[] {
  const urlRegex = /(https?:\/\/[^\s]+)/g;
  const parts: { type: 'text' | 'link'; content: string }[] = [];
  let lastIndex = 0;
  let match;

  while ((match = urlRegex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parts.push({ type: 'text', content: text.slice(lastIndex, match.index) });
    }
    parts.push({ type: 'link', content: match[1] });
    lastIndex = match.index + match[1].length;
  }

  if (lastIndex < text.length) {
    parts.push({ type: 'text', content: text.slice(lastIndex) });
  }

  return parts.length > 0 ? parts : [{ type: 'text', content: text }];
}

// Log Panel
const LogPanel: Component<{ appId: string; onClose: () => void }> = (props) => {
  const [store, actions] = useApp();
  let logRef: HTMLDivElement | undefined;
  let unlisten: UnlistenFn | undefined;

  const setupListener = async () => {
    unlisten = await listen<{ appId: string; line: string }>('app-output', (event) => {
      if (event.payload.appId === props.appId) {
        actions.appendLog(props.appId, event.payload.line);
        setTimeout(() => {
          if (logRef) logRef.scrollTop = logRef.scrollHeight;
        }, 10);
      }
    });
  };

  setupListener();
  onCleanup(() => { if (unlisten) unlisten(); });

  const logs = () => store.logs[props.appId] || [];
  const app = () => store.apps.find(a => a.id === props.appId);

  const handleLinkClick = async (url: string) => {
    try {
      await openUrl(url);
    } catch (e) {
      console.error('Failed to open URL:', e);
    }
  };

  const copyLogs = () => {
    const text = logs().join('\n');
    navigator.clipboard.writeText(text);
  };

  return (
    <div class="h-48 bg-black/60 border-t border-white/10 flex flex-col flex-shrink-0">
      <div class="flex items-center justify-between px-3 py-2 bg-black/40 border-b border-white/10">
        <div class="flex items-center gap-2">
          <Show when={app()?.isRunning}>
            <div class="w-2 h-2 rounded-full bg-green-400 animate-pulse" />
          </Show>
          <span class="text-xs font-medium text-white/70">{app()?.name || 'Log'}</span>
        </div>
        <div class="flex gap-1">
          <button onClick={copyLogs} class="text-xs px-2 py-1 rounded bg-white/5 hover:bg-white/10" title="Copy all">📋</button>
          <button onClick={() => actions.clearLogs(props.appId)} class="text-xs px-2 py-1 rounded bg-white/5 hover:bg-white/10">Clear</button>
          <button onClick={props.onClose} class="text-xs px-2 py-1 rounded bg-white/5 hover:bg-white/10">✕</button>
        </div>
      </div>
      <div
        ref={logRef}
        class="flex-1 p-3 overflow-auto font-mono text-xs leading-relaxed log-content"
      >
        <Show when={logs().length > 0} fallback={<p class="text-white/30 italic">Waiting for output...</p>}>
          <For each={logs()}>
            {(line) => {
              const isError = line.startsWith('[stderr]') || line.includes('❌') || line.includes('error') || line.includes('Error');
              const isSuccess = line.startsWith('✓');
              const isWarning = line.startsWith('⚠');

              return (
                <div class={`py-0.5 ${isError ? 'text-red-400' : isSuccess ? 'text-green-400' : isWarning ? 'text-yellow-400' : 'text-white/80'}`}>
                  <For each={parseLogLine(line)}>
                    {(part) => (
                      <Show
                        when={part.type === 'link'}
                        fallback={<span>{part.content}</span>}
                      >
                        <a
                          href="#"
                          onClick={(e) => { e.preventDefault(); handleLinkClick(part.content); }}
                          class="text-blue-400 hover:text-blue-300 underline cursor-pointer"
                        >
                          {part.content}
                        </a>
                      </Show>
                    )}
                  </For>
                </div>
              );
            }}
          </For>
        </Show>
      </div>
    </div>
  );
};

// App Modal
const AppModal: Component = () => {
  const [store, actions] = useApp();

  const isEdit = () => store.modal.type === 'edit';
  const editingApp = () => {
    const modal = store.modal;
    if (modal.type === 'edit') {
      return store.apps.find(a => a.id === modal.appId);
    }
    return null;
  };

  const [name, setName] = createSignal(editingApp()?.name || '');
  const [icon, setIcon] = createSignal<string | null>(editingApp()?.icon || null);
  const [appType, setAppType] = createSignal<AppType>(editingApp()?.appType || 'exe');
  const [execPath, setExecPath] = createSignal(editingApp()?.executablePath || '');
  const [workingDir, setWorkingDir] = createSignal(editingApp()?.workingDirectory || '');
  const [args, setArgs] = createSignal(editingApp()?.arguments || '');
  const [envVars, setEnvVars] = createSignal(editingApp()?.environmentVars || '');
  const [showAdvanced, setShowAdvanced] = createSignal(false);

  const isDark = () => store.settings.theme === 'dark';

  const placeholders: Record<AppType, string> = {
    exe: 'C:\\Path\\To\\App.exe',
    bat: 'C:\\Path\\To\\Script.bat',
    shell: 'python script.py --arg value',
  };

  const handleBrowseExe = async () => {
    const filters = appType() === 'shell'
      ? [{ name: 'All', extensions: ['*'] }]
      : appType() === 'bat'
        ? [{ name: 'Batch', extensions: ['bat', 'cmd'] }]
        : [{ name: 'Executable', extensions: ['exe'] }];
    const selected = await openDialog({ multiple: false, filters });
    if (selected) {
      const path = selected as string;
      setExecPath(path);

      // Auto-extract icon if it's an EXE
      if (path.toLowerCase().endsWith('.exe')) {
        try {
          const iconData = await invoke<string>('extract_exe_icon', { exePath: path });
          setIcon(iconData);
        } catch (err) {
          console.error('Failed to auto-extract icon:', err);
        }
      }
    }
  };

  const handleBrowseIcon = async () => {
    const selected = await openDialog({
      multiple: false,
      filters: [
        { name: 'Images & Executables', extensions: ['png', 'jpg', 'jpeg', 'ico', 'svg', 'exe'] },
        { name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'ico', 'svg'] },
        { name: 'Executables', extensions: ['exe'] },
      ],
    });
    if (selected) {
      const path = selected as string;
      if (path.toLowerCase().endsWith('.exe')) {
        try {
          const iconData = await invoke<string>('extract_exe_icon', { exePath: path });
          setIcon(iconData);
        } catch (err) {
          console.error('Failed to extract icon:', err);
          setIcon(null);
        }
      } else {
        setIcon(`file://${path}`);
      }
    }
  };

  const handleBrowseWorkDir = async () => {
    const selected = await openDialog({ directory: true });
    if (selected) setWorkingDir(selected as string);
  };

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    if (!name().trim() || !execPath().trim()) return;

    let isRunning = false;
    try {
      if (appType() === 'exe' || execPath().toLowerCase().endsWith('.exe')) {
        isRunning = await invoke<boolean>('check_process_running', { exePath: execPath() });
      }
    } catch (e) {
      console.error('Failed to check process:', e);
    }

    const data = {
      name: name(), icon: icon(), appType: appType(),
      executablePath: execPath(), workingDirectory: workingDir(),
      arguments: args(), environmentVars: envVars(),
      isRunning: isRunning,
    };

    if (isEdit() && store.modal.type === 'edit') {
      actions.updateApp(store.modal.appId, data);
    } else {
      actions.addApp(data);
    }
    actions.closeModal();
  };

  // Styles
  const modalClass = isDark() ? 'bg-slate-800 border-white/10 text-white' : 'bg-white border-gray-200 text-gray-900';
  const inputClass = isDark()
    ? 'bg-white/5 border-white/10 focus:border-blue-500/50 placeholder-white/20'
    : 'bg-gray-50 border-gray-200 focus:border-blue-500 focus:bg-white placeholder-gray-400';
  const labelClass = isDark() ? 'text-white/50' : 'text-gray-500';
  const btnSecondaryClass = isDark() ? 'bg-white/5 hover:bg-white/10 border-white/10' : 'bg-gray-100 hover:bg-gray-200 border-gray-200 text-gray-700';
  const iconBtnClass = isDark() ? 'bg-white/5 border-white/20 hover:border-white/40' : 'bg-gray-50 border-gray-200 hover:border-gray-400';

  return (
    <div class="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 p-4" onClick={actions.closeModal}>
      <div class={`${modalClass} rounded-2xl p-5 w-full max-w-md shadow-2xl border transition-colors duration-300`} onClick={e => e.stopPropagation()}>
        <h2 class="text-lg font-semibold mb-4">{isEdit() ? '✏️ Sửa' : '➕ Thêm'} ứng dụng</h2>

        <form onSubmit={handleSubmit} class="space-y-3">
          {/* Icon & Name */}
          <div class="flex gap-3">
            <button type="button" onClick={handleBrowseIcon}
              class={`w-14 h-14 flex-shrink-0 rounded-xl border-2 border-dashed flex items-center justify-center overflow-hidden transition-colors ${iconBtnClass}`}>
              <Show when={icon()} fallback={<span class={`text-xl ${isDark() ? 'text-white/40' : 'text-gray-400'}`}>📷</span>}>
                <img src={icon()!} class="w-full h-full object-cover" />
              </Show>
            </button>
            <div class="flex-1">
              <label class={`text-xs mb-1 block ${labelClass}`}>Tên</label>
              <input type="text" value={name()} onInput={e => setName(e.currentTarget.value)}
                placeholder="My App" required
                class={`w-full px-3 py-2 rounded-lg border outline-none text-sm transition-colors ${inputClass}`} />
            </div>
          </div>

          {/* App Type */}
          <div>
            <label class={`text-xs mb-1 block ${labelClass}`}>Loại</label>
            <div class="grid grid-cols-3 gap-2">
              <For each={(['exe', 'bat', 'shell'] as AppType[])}>
                {(t) => (
                  <button type="button"
                    class={`py-2 rounded-lg text-xs font-medium border transition-colors
                      ${appType() === t
                        ? 'bg-blue-500/20 border-blue-500/50 text-blue-500'
                        : isDark()
                          ? 'bg-white/5 border-white/10 text-white/60 hover:bg-white/10'
                          : 'bg-gray-50 border-gray-200 text-gray-600 hover:bg-gray-100'}`}
                    onClick={() => setAppType(t)}>
                    {t === 'exe' ? '🖥️ EXE' : t === 'bat' ? '📄 BAT' : '💻 Shell'}
                  </button>
                )}
              </For>
            </div>
          </div>

          {/* Path */}
          <div>
            <label class={`text-xs mb-1 block ${labelClass}`}>{appType() === 'shell' ? 'Lệnh' : 'Đường dẫn'}</label>
            <div class="flex gap-2">
              <input type="text" value={execPath()} onInput={e => setExecPath(e.currentTarget.value)}
                placeholder={placeholders[appType()]} required
                class={`flex-1 px-3 py-2 rounded-lg border outline-none text-sm transition-colors ${inputClass}`} />
              <Show when={appType() !== 'shell'}>
                <button type="button" onClick={handleBrowseExe} class={`px-3 py-2 rounded-lg border transition-colors ${btnSecondaryClass}`}>📁</button>
              </Show>
            </div>
          </div>

          {/* Advanced */}
          <div class={`border-t pt-3 ${isDark() ? 'border-white/10' : 'border-gray-200'}`}>
            <button type="button" onClick={() => setShowAdvanced(!showAdvanced())}
              class={`flex items-center gap-2 text-xs w-full transition-colors ${isDark() ? 'text-white/50 hover:text-white/70' : 'text-gray-500 hover:text-gray-700'}`}>
              <span class={`transition-transform ${showAdvanced() ? 'rotate-180' : ''}`}>▼</span>
              Nâng cao
            </button>
            <Show when={showAdvanced()}>
              <div class="mt-3 space-y-3">
                <div>
                  <label class={`text-xs mb-1 block ${labelClass}`}>Thư mục làm việc</label>
                  <div class="flex gap-2">
                    <input type="text" value={workingDir()} onInput={e => setWorkingDir(e.currentTarget.value)}
                      placeholder="C:\Path" class={`flex-1 px-3 py-2 rounded-lg border outline-none text-sm transition-colors ${inputClass}`} />
                    <button type="button" onClick={handleBrowseWorkDir} class={`px-3 py-2 rounded-lg border transition-colors ${btnSecondaryClass}`}>📁</button>
                  </div>
                </div>
                <div>
                  <label class={`text-xs mb-1 block ${labelClass}`}>Tham số</label>
                  <input type="text" value={args()} onInput={e => setArgs(e.currentTarget.value)}
                    placeholder="--port 8080" class={`w-full px-3 py-2 rounded-lg border outline-none text-sm transition-colors ${inputClass}`} />
                </div>
                <div>
                  <label class={`text-xs mb-1 block ${labelClass}`}>Biến môi trường (KEY=value)</label>
                  <textarea value={envVars()} onInput={e => setEnvVars(e.currentTarget.value)}
                    placeholder="NODE_ENV=production" rows={2}
                    class={`w-full px-3 py-2 rounded-lg border outline-none text-sm resize-none transition-colors ${inputClass}`} />
                </div>
              </div>
            </Show>
          </div>

          {/* Buttons */}
          <div class="flex gap-2 pt-2">
            <button type="button" onClick={actions.closeModal}
              class={`flex-1 py-2.5 rounded-lg border text-sm transition-colors ${btnSecondaryClass}`}>Hủy</button>
            <button type="submit"
              class="flex-1 py-2.5 rounded-lg bg-gradient-to-r from-blue-500 to-purple-600 text-white text-sm font-medium hover:shadow-lg hover:shadow-blue-500/25 transition-all">
              {isEdit() ? 'Lưu' : 'Thêm'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};

// Settings Modal
const SettingsModal: Component = () => {
  const [store, actions] = useApp();
  const isDark = () => store.settings.theme === 'dark';

  const modalClass = isDark() ? 'bg-slate-800 border-white/10 text-white' : 'bg-white border-gray-200 text-gray-900';
  const itemClass = isDark() ? 'bg-white/5 border-white/10 hover:bg-white/10' : 'bg-gray-50 border-gray-200 hover:bg-gray-100';
  const textSubClass = isDark() ? 'text-white/50' : 'text-gray-500';
  const btnClass = isDark() ? 'bg-white/5 hover:bg-white/10' : 'bg-gray-100 hover:bg-gray-200 text-gray-700';

  return (
    <div class="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 p-4" onClick={actions.closeModal}>
      <div class={`${modalClass} rounded-2xl p-5 w-full max-w-sm shadow-2xl border transition-colors duration-300`} onClick={e => e.stopPropagation()}>
        <h2 class="text-lg font-semibold mb-4">⚙️ Cài đặt</h2>

        <label class={`flex items-center justify-between p-3 rounded-xl border cursor-pointer transition-colors ${itemClass}`}>
          <div>
            <p class="text-sm font-medium">Thu nhỏ vào Tray</p>
            <p class={`text-xs ${textSubClass}`}>Khi đóng app, thu vào system tray</p>
          </div>
          <input type="checkbox" checked={store.settings.minimizeToTray}
            onChange={e => actions.updateSettings({ minimizeToTray: e.currentTarget.checked })}
            class="w-5 h-5 rounded accent-blue-500" />
        </label>

        <div class={`mt-4 p-3 rounded-xl border ${itemClass}`}>
          <p class="text-sm font-medium">AppCtrl v1.0.1</p>
          <p class={`text-xs ${textSubClass}`}>Simple App Manager</p>
        </div>

        <button onClick={actions.closeModal} class={`w-full mt-4 py-2.5 rounded-lg text-sm transition-colors ${btnClass}`}>Đóng</button>
      </div>
    </div>
  );
};

export default App;
