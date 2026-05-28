import { Component, createSignal, createEffect, onMount, For, Show, onCleanup } from 'solid-js';
import { invoke } from '@tauri-apps/api/core';
import { useApp } from './store/appStore';
import { ask, message } from '@tauri-apps/plugin-dialog';
import type { DiskInfo, FileInfo } from './types';

// Global cache for icons (persists across modal openings)
const iconCache: Record<string, string> = {};

const getFileIcon = async (file: { path: string; isDir: boolean; extension: string }) => {
    if (file.isDir) {
        if (iconCache['__folder__']) return iconCache['__folder__'];
        try {
            const icon = await invoke<string>('get_system_icon', { path: file.path, isDir: true, useAttr: false });
            iconCache['__folder__'] = icon;
            return icon;
        } catch (e) {
            return '';
        }
    }
    
    const ext = file.extension.toLowerCase();
    // Dynamic icons that depend on the specific file content or exact executable
    const isDynamic = ['exe', 'lnk', 'ico', 'png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp', 'mp4', 'avi', 'mkv'].includes(ext);
    
    if (!isDynamic && ext) {
        const cacheKey = `ext_${ext}`;
        if (iconCache[cacheKey]) return iconCache[cacheKey];
        try {
            const dummyPath = `dummy.${ext}`;
            const icon = await invoke<string>('get_system_icon', { path: dummyPath, isDir: false, useAttr: true });
            iconCache[cacheKey] = icon;
            return icon;
        } catch (e) {
            return '';
        }
    }
    
    try {
        return await invoke<string>('get_system_icon', { path: file.path, isDir: false, useAttr: false });
    } catch (e) {
        return '';
    }
};

const FileIcon: Component<{ file: { path: string; isDir: boolean; extension: string } }> = (props) => {
    const [iconSrc, setIconSrc] = createSignal<string>('');

    createEffect(async () => {
        const src = await getFileIcon(props.file);
        setIconSrc(src);
    });

    return (
        <Show 
            when={iconSrc()} 
            fallback={<span class="text-lg flex-shrink-0">{props.file.isDir ? '📁' : '📄'}</span>}
        >
            <img src={iconSrc()} class="w-5 h-5 object-contain flex-shrink-0" alt="" />
        </Show>
    );
};

const DiskIcon: Component<{ path: string }> = (props) => {
    const [iconSrc, setIconSrc] = createSignal<string>('');

    createEffect(async () => {
        if (iconCache['__disk__']) {
            setIconSrc(iconCache['__disk__']);
            return;
        }
        try {
            const icon = await invoke<string>('get_system_icon', { path: props.path, isDir: true, useAttr: false });
            iconCache['__disk__'] = icon;
            setIconSrc(icon);
        } catch (e) {
            console.error(e);
        }
    });

    return (
        <Show 
            when={iconSrc()} 
            fallback={<span class="text-2xl">💽</span>}
        >
            <img src={iconSrc()} class="w-10 h-10 object-contain" alt="" />
        </Show>
    );
};

export const FileExplorerModal: Component = () => {
    const [store, actions] = useApp();
    const [currentPath, setCurrentPath] = createSignal<string>('');
    const [disks, setDisks] = createSignal<DiskInfo[]>([]);
    const [files, setFiles] = createSignal<FileInfo[]>([]);
    const [searchQuery, setSearchQuery] = createSignal<string>('');
    const [loading, setLoading] = createSignal<boolean>(true);
    const [addressInput, setAddressInput] = createSignal<string>('');
    const [selectedItemPath, setSelectedItemPath] = createSignal<string | null>(null);
    
    // Clipboard: { path: string, isDir: boolean, name: string }
    const [clipboard, setClipboard] = createSignal<{ path: string; isDir: boolean; name: string } | null>(null);
    
    // Context Menu: { x: number, y: number, show: boolean, item?: FileInfo }
    const [contextMenu, setContextMenu] = createSignal<{ x: number; y: number; show: boolean; item?: FileInfo }>({
        x: 0,
        y: 0,
        show: false
    });

    interface LockProcessInfo {
        pid: number;
        name: string;
    }

    const [lockProcesses, setLockProcesses] = createSignal<LockProcessInfo[]>([]);
    const [showLockModal, setShowLockModal] = createSignal<boolean>(false);
    const [showForceDeleteModal, setShowForceDeleteModal] = createSignal<boolean>(false);
    const [activeItem, setActiveItem] = createSignal<FileInfo | null>(null);
    const [lockModalLoading, setLockModalLoading] = createSignal<boolean>(false);

    const isDark = () => store.settings.theme === 'dark';

    const checkLockProcesses = async (item: FileInfo) => {
        setLockModalLoading(true);
        setActiveItem(item);
        setLockProcesses([]);
        setShowLockModal(true);
        try {
            const list = await invoke<LockProcessInfo[]>('get_file_lock_processes', { path: item.path });
            setLockProcesses(list);
        } catch (e) {
            console.error(e);
            await message(`Lỗi khi lấy tiến trình khóa: ${e}`, { title: 'Lỗi', kind: 'error' });
            setShowLockModal(false);
        } finally {
            setLockModalLoading(false);
        }
    };

    const handleOpenForceDeleteConfirm = async (item: FileInfo) => {
        setLockModalLoading(true);
        setActiveItem(item);
        setLockProcesses([]);
        setShowForceDeleteModal(true);
        try {
            const list = await invoke<LockProcessInfo[]>('get_file_lock_processes', { path: item.path });
            setLockProcesses(list);
        } catch (e) {
            console.error(e);
        } finally {
            setLockModalLoading(false);
        }
    };

    const handleForceDelete = async () => {
        const item = activeItem();
        if (!item) return;
        
        setLoading(true);
        setShowForceDeleteModal(false);
        try {
            await invoke('force_delete_file', { path: item.path });
            if (currentPath()) {
                await loadFiles(currentPath());
            }
            await message(`Đã xóa cưỡng ép thành công "${item.name}"`, { title: 'Thành công', kind: 'info' });
        } catch (e) {
            await message(`Lỗi khi xóa cưỡng ép: ${e}`, { title: 'Lỗi', kind: 'error' });
        } finally {
            setLoading(false);
        }
    };

    const loadDisks = async () => {
        setLoading(true);
        try {
            const list = await invoke<DiskInfo[]>('get_disks');
            setDisks(list);
            setCurrentPath('');
            setAddressInput('');
        } catch (e) {
            console.error(e);
            await message(`Không thể lấy danh sách ổ đĩa: ${e}`, { title: 'Lỗi', kind: 'error' });
        } finally {
            setLoading(false);
        }
    };

    const loadFiles = async (path: string) => {
        setLoading(true);
        setSearchQuery('');
        setSelectedItemPath(null);
        try {
            const list = await invoke<FileInfo[]>('read_directory', { path });
            setFiles(list);
            setCurrentPath(path);
            setAddressInput(path);
        } catch (e) {
            console.error(e);
            await message(`Không thể mở thư mục: ${e}`, { title: 'Lỗi', kind: 'error' });
            // revert addressInput
            setAddressInput(currentPath());
        } finally {
            setLoading(false);
        }
    };

    onMount(() => {
        loadDisks();
        
        const closeMenu = () => setContextMenu(prev => ({ ...prev, show: false }));
        window.addEventListener('click', closeMenu, { capture: true });
        onCleanup(() => window.removeEventListener('click', closeMenu, { capture: true }));
    });

    const handleNavigate = (path: string) => {
        if (!path.trim()) {
            loadDisks();
        } else {
            loadFiles(path.trim());
        }
    };

    const handleBack = () => {
        if (!currentPath()) return;
        
        let path = currentPath().trim();
        if (path.endsWith('\\') && path.length > 3) {
            path = path.slice(0, -1);
        }
        
        const lastSlash = path.lastIndexOf('\\');
        if (lastSlash === -1 || path.length <= 3) {
            loadDisks();
        } else {
            const parent = path.substring(0, lastSlash);
            const finalParent = parent.length === 2 && parent.endsWith(':') ? parent + '\\' : parent;
            loadFiles(finalParent);
        }
    };

    const filteredFiles = () => {
        const q = searchQuery().toLowerCase().trim();
        if (!q) return files();
        return files().filter(f => f.name.toLowerCase().includes(q));
    };

    const formatBytes = (bytes: number): string => {
        if (bytes === 0) return '0 Bytes';
        const k = 1024;
        const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    };

    const formatDiskSize = (bytes: number): string => {
        const gb = bytes / (1024 * 1024 * 1024);
        return `${gb.toFixed(1)} GB`;
    };

    const handleContextMenu = (e: MouseEvent, item?: FileInfo) => {
        e.preventDefault();
        e.stopPropagation();
        setContextMenu({
            x: e.clientX,
            y: e.clientY,
            show: true,
            item
        });
    };

    const handleOpenInExplorer = async (item: FileInfo) => {
        try {
            await invoke('open_in_explorer', { path: item.path });
        } catch (e) {
            await message(`Không thể mở trong Explorer: ${e}`, { title: 'Lỗi', kind: 'error' });
        }
    };

    const handleCopy = (item: FileInfo) => {
        setClipboard({
            path: item.path,
            isDir: item.isDir,
            name: item.name
        });
    };

    const handlePaste = async (targetDir: string) => {
        const clip = clipboard();
        if (!clip) return;
        
        setLoading(true);
        try {
            await invoke('paste_file', { src: clip.path, destDir: targetDir });
            // reload
            if (currentPath()) {
                await loadFiles(currentPath());
            }
        } catch (e) {
            await message(`Lỗi khi dán file: ${e}`, { title: 'Lỗi', kind: 'error' });
        } finally {
            setLoading(false);
        }
    };

    const handleDelete = async (item: FileInfo) => {
        const confirmed = await ask(`Bạn có chắc chắn muốn xóa "${item.name}"? Hành động này không thể hoàn tác.`, {
            title: 'Xác nhận xóa',
            kind: 'warning'
        });
        
        if (confirmed) {
            setLoading(true);
            try {
                await invoke('delete_file', { path: item.path });
                if (currentPath()) {
                    await loadFiles(currentPath());
                }
            } catch (e) {
                await message(`Lỗi khi xóa file: ${e}`, { title: 'Lỗi', kind: 'error' });
            } finally {
                setLoading(false);
            }
        }
    };

    // Styling constants
    const modalClass = isDark() 
        ? 'bg-slate-900 border-white/10 text-white' 
        : 'bg-white border-gray-200 text-gray-900';
    const inputClass = isDark()
        ? 'bg-white/5 border-white/10 focus:border-blue-500/50 placeholder-white/20'
        : 'bg-gray-50 border-gray-200 focus:border-blue-500 focus:bg-white placeholder-gray-400';
    const listHeaderClass = isDark() ? 'border-white/10 text-white/50 bg-black/20' : 'border-gray-200 text-gray-500 bg-gray-50';
    const itemClass = isDark() 
        ? 'hover:bg-white/5 border-white/5' 
        : 'hover:bg-gray-50 border-gray-100';
    const selectedItemClass = isDark() ? 'bg-blue-500/20' : 'bg-blue-50';
    const contextMenuClass = isDark() ? 'bg-slate-800 border-white/10 text-white shadow-2xl' : 'bg-white border-gray-200 text-gray-800 shadow-xl';

    return (
        <div class="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 p-4" onClick={actions.closeModal}>
            <div 
                class={`${modalClass} rounded-2xl p-5 w-full max-w-3xl h-[80vh] shadow-2xl border transition-colors duration-300 flex flex-col`}
                onClick={e => e.stopPropagation()}
            >
                {/* Header */}
                <div class="flex items-center justify-between mb-4 flex-shrink-0">
                    <div class="flex items-center gap-2">
                        <span class="text-xl">📁</span>
                        <h2 class="text-lg font-semibold">Mini File Explorer</h2>
                    </div>
                    <button 
                        onClick={actions.closeModal} 
                        class={`p-2 rounded-lg transition-colors ${isDark() ? 'hover:bg-white/10' : 'hover:bg-gray-100'}`} 
                        title="Đóng"
                    >
                        ❌
                    </button>
                </div>

                {/* Path bar and search bar */}
                <div class="flex gap-2 mb-4 flex-shrink-0">
                    <button 
                        onClick={handleBack} 
                        disabled={!currentPath()}
                        class={`px-3 py-2 rounded-lg border transition-all flex items-center justify-center
                            ${!currentPath() 
                                ? 'opacity-40 cursor-not-allowed border-gray-200 dark:border-white/5' 
                                : isDark() 
                                    ? 'bg-white/5 border-white/10 hover:bg-white/10 active:scale-95' 
                                    : 'bg-gray-100 border-gray-200 hover:bg-gray-200 active:scale-95 text-gray-700'}`}
                        title="Quay lại"
                    >
                        ⬅️
                    </button>
                    
                    <input 
                        type="text" 
                        value={addressInput()}
                        onInput={e => setAddressInput(e.currentTarget.value)}
                        onKeyDown={e => e.key === 'Enter' && handleNavigate(addressInput())}
                        placeholder="Nhập đường dẫn thư mục..."
                        class={`flex-1 px-3 py-2 rounded-lg border outline-none text-sm transition-colors font-mono ${inputClass}`}
                    />

                    <input 
                        type="text" 
                        value={searchQuery()}
                        onInput={e => setSearchQuery(e.currentTarget.value)}
                        placeholder="Tìm kiếm..."
                        class={`w-48 px-3 py-2 rounded-lg border outline-none text-sm transition-colors ${inputClass}`}
                    />
                </div>

                {/* Main Content Area */}
                <div 
                    class="flex-1 overflow-auto rounded-xl border relative bg-black/10 dark:bg-black/20 border-gray-200 dark:border-white/10"
                    onContextMenu={e => currentPath() && handleContextMenu(e)}
                >
                    <Show when={!loading()} fallback={
                        <div class="absolute inset-0 flex items-center justify-center bg-black/5 dark:bg-black/10 backdrop-blur-sm z-10">
                            <div class="flex flex-col items-center gap-2">
                                <div class="w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin"></div>
                                <span class="text-xs text-gray-400">Đang tải...</span>
                            </div>
                        </div>
                    }>
                        {/* Root View - Disks */}
                        <Show when={!currentPath()}>
                            <div class="p-6 grid grid-cols-1 md:grid-cols-2 gap-4">
                                <For each={disks()}>
                                    {(disk) => {
                                        const usedSpace = disk.totalSpace - disk.freeSpace;
                                        const usedPercent = disk.totalSpace > 0 ? (usedSpace / disk.totalSpace) * 100 : 0;
                                        const progressColor = usedPercent > 90 
                                            ? 'bg-gradient-to-r from-red-500 to-rose-600' 
                                            : 'bg-gradient-to-r from-blue-500 to-purple-600';
                                        
                                        return (
                                            <div 
                                                onClick={() => handleNavigate(disk.name)}
                                                class={`p-4 rounded-xl border cursor-pointer transition-all duration-300 hover:scale-[1.02] shadow-sm flex items-center gap-4
                                                    ${isDark() 
                                                        ? 'bg-white/5 border-white/10 hover:bg-white/10 hover:border-blue-500/30' 
                                                        : 'bg-white border-gray-200 hover:bg-gray-50 hover:border-blue-500/30 shadow-gray-100'}`}
                                            >
                                                <DiskIcon path={disk.name} />
                                                <div class="flex-1 min-w-0">
                                                    <p class="font-semibold text-sm">{disk.name}</p>
                                                    <div class="flex justify-between text-xs text-gray-400 mt-1 mb-1.5">
                                                        <span>{formatDiskSize(disk.freeSpace)} trống / {formatDiskSize(disk.totalSpace)}</span>
                                                        <span>Đã dùng {formatDiskSize(usedSpace)} ({usedPercent.toFixed(1)}%)</span>
                                                    </div>
                                                    {/* Progress bar */}
                                                    <div class="w-full h-2 rounded-full bg-gray-200 dark:bg-white/10 overflow-hidden">
                                                        <div 
                                                            class={`h-full rounded-full transition-all duration-500 ${progressColor}`} 
                                                            style={{ width: `${usedPercent}%` }}
                                                        />
                                                    </div>
                                                </div>
                                            </div>
                                        );
                                    }}
                                </For>
                            </div>
                        </Show>

                        {/* Folder View - Files & Folders */}
                        <Show when={currentPath()}>
                            <div class="flex flex-col w-full h-full">
                                {/* Table Header */}
                                <div class={`flex items-center text-xs font-semibold uppercase tracking-wider py-2 px-4 border-b ${listHeaderClass}`}>
                                    <div class="w-[45%]">Tên</div>
                                    <div class="w-[30%]">Ngày sửa</div>
                                    <div class="w-[25%] text-right font-sans">Dung lượng</div>
                                </div>
                                
                                <div class="flex-1 overflow-y-auto">
                                    <Show when={filteredFiles().length > 0} fallback={
                                        <div class="text-center py-20 text-sm text-gray-400">Thư mục trống</div>
                                    }>
                                        <For each={filteredFiles()}>
                                            {(file) => (
                                                <div 
                                                    onClick={() => setSelectedItemPath(file.path)}
                                                    onDblClick={() => file.isDir ? handleNavigate(file.path) : handleOpenInExplorer(file)}
                                                    onContextMenu={(e) => handleContextMenu(e, file)}
                                                    class={`flex items-center py-2 px-4 border-b text-sm cursor-pointer select-none transition-colors
                                                        ${itemClass}
                                                        ${selectedItemPath() === file.path ? selectedItemClass : ''}`}
                                                >
                                                    {/* Name Column */}
                                                    <div class="w-[45%] flex items-center gap-2.5 min-w-0 pr-4">
                                                        <FileIcon file={file} />
                                                        <span class="truncate" title={file.name}>{file.name}</span>
                                                    </div>
                                                    
                                                    {/* Date Modified Column */}
                                                    <div class="w-[30%] text-xs text-gray-400">
                                                        {file.modified > 0 
                                                            ? new Date(file.modified * 1000).toLocaleString('vi-VN', {
                                                                year: 'numeric',
                                                                month: '2-digit',
                                                                day: '2-digit',
                                                                hour: '2-digit',
                                                                minute: '2-digit'
                                                            })
                                                            : '-'
                                                        }
                                                    </div>
                                                    
                                                    {/* Size Column */}
                                                    <div class="w-[25%] text-right text-xs text-gray-400 pr-2">
                                                        {formatBytes(file.size)}
                                                    </div>
                                                </div>
                                            )}
                                        </For>
                                    </Show>
                                </div>
                            </div>
                        </Show>
                    </Show>
                </div>

                {/* Footer Info / Status bar */}
                <div class="flex items-center justify-between mt-3 text-xs text-gray-400 flex-shrink-0 px-1">
                    <div>
                        <Show when={currentPath()}>
                            <span>Tổng số: {filteredFiles().length} mục</span>
                        </Show>
                    </div>
                    <div>
                        <Show when={clipboard()}>
                            <span class="text-blue-500 bg-blue-500/10 px-2 py-0.5 rounded-full border border-blue-500/20 animate-pulse">
                                📋 Đang sao chép: {clipboard()?.name}
                            </span>
                        </Show>
                    </div>
                </div>

                {/* Custom Context Menu */}
                <Show when={contextMenu().show}>
                    <div 
                        class={`fixed rounded-xl border py-1.5 min-w-[170px] z-50 shadow-2xl backdrop-blur-md flex flex-col font-medium text-xs ${contextMenuClass}`}
                        style={{ top: `${contextMenu().y}px`, left: `${contextMenu().x}px` }}
                        onClick={e => e.stopPropagation()}
                    >
                        {/* Context-specific options */}
                        <Show when={contextMenu().item}>
                            <button 
                                onClick={() => { handleOpenInExplorer(contextMenu().item!); setContextMenu(prev => ({ ...prev, show: false })); }}
                                class="px-3.5 py-2 text-left hover:bg-blue-500 hover:text-white transition-colors flex items-center gap-2"
                            >
                                🔍 Mở bằng Explorer hệ thống
                            </button>
                            <button 
                                onClick={() => { handleCopy(contextMenu().item!); setContextMenu(prev => ({ ...prev, show: false })); }}
                                class="px-3.5 py-2 text-left hover:bg-blue-500 hover:text-white transition-colors flex items-center gap-2"
                            >
                                📋 Sao chép
                            </button>
                            <button 
                                onClick={() => { checkLockProcesses(contextMenu().item!); setContextMenu(prev => ({ ...prev, show: false })); }}
                                class="px-3.5 py-2 text-left hover:bg-blue-500 hover:text-white transition-colors flex items-center gap-2"
                            >
                                🔒 Tiến trình chiếm dụng
                            </button>
                        </Show>

                        {/* Paste is available if clipboard has items */}
                        <Show when={clipboard()}>
                            <button 
                                onClick={() => { 
                                    const target = contextMenu().item?.isDir 
                                        ? contextMenu().item!.path 
                                        : currentPath(); 
                                    if (target) handlePaste(target);
                                    setContextMenu(prev => ({ ...prev, show: false })); 
                                }}
                                class="px-3.5 py-2 text-left hover:bg-blue-500 hover:text-white transition-colors flex items-center gap-2"
                            >
                                📥 Dán tại đây
                            </button>
                        </Show>

                        {/* Delete is available on items */}
                        <Show when={contextMenu().item}>
                            <div class="h-[1px] my-1 bg-gray-200 dark:bg-white/10" />
                            <button 
                                onClick={() => { handleDelete(contextMenu().item!); setContextMenu(prev => ({ ...prev, show: false })); }}
                                class="px-3.5 py-2 text-left hover:bg-red-500 hover:text-white text-red-500 transition-colors flex items-center gap-2"
                            >
                                🗑️ Xóa
                            </button>
                            <button 
                                onClick={() => { handleOpenForceDeleteConfirm(contextMenu().item!); setContextMenu(prev => ({ ...prev, show: false })); }}
                                class="px-3.5 py-2 text-left hover:bg-red-600 hover:text-white text-red-600 transition-colors flex items-center gap-2"
                            >
                                💥 Xoá bắt buộc
                            </button>
                        </Show>
                        
                        {/* Fallback option when right-clicking on whitespace and nothing is copied */}
                        <Show when={!contextMenu().item && !clipboard()}>
                            <div class="px-3.5 py-2 text-gray-400 text-center italic cursor-default">
                                Không có tùy chọn
                            </div>
                        </Show>
                    </div>
                </Show>

                {/* Modal Xem tiến trình khóa (Process Used) */}
                <Show when={showLockModal()}>
                    <div class="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 p-4" onClick={() => setShowLockModal(false)}>
                        <div 
                            class={`${modalClass} rounded-2xl p-5 w-full max-w-md shadow-2xl border transition-colors duration-300 flex flex-col max-h-[60vh]`}
                            onClick={e => e.stopPropagation()}
                        >
                            <div class="flex items-center justify-between mb-4">
                                <h3 class="font-semibold text-sm flex items-center gap-2">
                                    🔒 Tiến trình đang chiếm dụng
                                </h3>
                                <button onClick={() => setShowLockModal(false)} class="text-xs">✕</button>
                            </div>
                            
                            <p class="text-xs text-gray-400 mb-3 truncate">
                                Tài nguyên: <span class="font-mono">{activeItem()?.name}</span>
                            </p>

                            <div class="flex-1 overflow-auto space-y-2 py-2 min-h-[150px]">
                                <Show when={!lockModalLoading()} fallback={
                                    <div class="flex items-center justify-center h-full text-xs text-gray-400">Đang quét tiến trình...</div>
                                }>
                                    <Show when={lockProcesses().length > 0} fallback={
                                        <div class="text-center py-8 text-xs text-green-500 bg-green-500/5 border border-green-500/10 rounded-xl">
                                            ✅ Không có tiến trình nào đang khóa file/thư mục này.
                                        </div>
                                    }>
                                        <div class="border rounded-xl overflow-hidden border-gray-200 dark:border-white/10 bg-black/5 dark:bg-black/20">
                                            <div class="grid grid-cols-12 text-xs font-semibold py-2 px-3 bg-black/10 dark:bg-black/40 text-gray-400 border-b border-gray-200 dark:border-white/10">
                                                <span class="col-span-6">Tên tiến trình</span>
                                                <span class="col-span-3 text-center">PID</span>
                                                <span class="col-span-3 text-right">Thao tác</span>
                                            </div>
                                            <For each={lockProcesses()}>
                                                {(proc) => (
                                                    <div class="grid grid-cols-12 items-center text-xs py-2.5 px-3 hover:bg-black/5 dark:hover:bg-white/5 border-b last:border-0 border-gray-100 dark:border-white/5 font-mono">
                                                        <span class="col-span-6 truncate font-semibold text-blue-500 dark:text-blue-400" title={proc.name}>
                                                            {proc.name}
                                                        </span>
                                                        <span class="col-span-3 text-center text-gray-700 dark:text-gray-300">{proc.pid}</span>
                                                        <span class="col-span-3 text-right font-sans">
                                                            <button 
                                                                onClick={async (e) => {
                                                                    e.stopPropagation();
                                                                    try {
                                                                        await invoke('kill_process_by_pid', { pid: proc.pid });
                                                                        if (activeItem()) {
                                                                            const list = await invoke<LockProcessInfo[]>('get_file_lock_processes', { path: activeItem()!.path });
                                                                            setLockProcesses(list);
                                                                        }
                                                                    } catch (err) {
                                                                        await message(`Không thể tắt tiến trình: ${err}`, { title: 'Lỗi', kind: 'error' });
                                                                    }
                                                                }}
                                                                class="px-2 py-0.5 rounded bg-red-500/10 text-red-500 hover:bg-red-500 hover:text-white transition-colors text-[10px] font-semibold border border-red-500/20 active:scale-95"
                                                            >
                                                                Kill
                                                            </button>
                                                        </span>
                                                    </div>
                                                )}
                                            </For>
                                        </div>
                                    </Show>
                                </Show>
                            </div>

                            <div class="flex justify-end mt-4">
                                <button 
                                    onClick={() => setShowLockModal(false)}
                                    class={`px-4 py-2 rounded-lg text-xs font-medium border transition-colors
                                        ${isDark() ? 'bg-white/5 hover:bg-white/10 border-white/10' : 'bg-gray-100 hover:bg-gray-200 border-gray-200 text-gray-700'}`}
                                >
                                    Đóng
                                </button>
                            </div>
                        </div>
                    </div>
                </Show>

                {/* Modal Xác nhận xóa cưỡng ép (Force Delete) */}
                <Show when={showForceDeleteModal()}>
                    <div class="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 p-4" onClick={() => setShowForceDeleteModal(false)}>
                        <div 
                            class={`${modalClass} rounded-2xl p-5 w-full max-w-md shadow-2xl border border-red-500/20 transition-colors duration-300 flex flex-col max-h-[70vh]`}
                            onClick={e => e.stopPropagation()}
                        >
                            <div class="flex items-center justify-between mb-3 text-red-500">
                                <h3 class="font-semibold text-sm flex items-center gap-2">
                                    ⚠️ Xác nhận xóa cưỡng ép (Force Delete)
                                </h3>
                                <button onClick={() => setShowForceDeleteModal(false)} class="text-xs">✕</button>
                            </div>
                            
                            <div class="text-xs space-y-2 mb-4">
                                <p class="text-gray-400">
                                    Bạn có chắc chắn muốn xóa cưỡng ép tài nguyên sau?
                                </p>
                                <p class="font-mono font-semibold truncate bg-black/10 dark:bg-black/30 p-2 rounded border border-gray-200 dark:border-white/5">
                                    {activeItem()?.name}
                                </p>
                                <p class="text-amber-500 dark:text-amber-400 bg-amber-500/10 border border-amber-500/20 p-2.5 rounded-lg leading-relaxed">
                                    ℹ️ Hệ thống sẽ tự động tắt (kill) các tiến trình đang chiếm dụng bên dưới, sau đó chuyển file/thư mục này vào Thùng rác (Recycle Bin).
                                </p>
                            </div>

                            <div class="flex-1 overflow-auto space-y-2 min-h-[120px]">
                                <Show when={!lockModalLoading()} fallback={
                                    <div class="flex items-center justify-center h-full text-xs text-gray-400">Đang quét tiến trình khóa...</div>
                                }>
                                    <Show when={lockProcesses().length > 0} fallback={
                                        <div class="text-center py-6 text-xs text-gray-400 bg-black/5 dark:bg-black/20 rounded-xl italic">
                                            Không phát hiện tiến trình nào đang chiếm dụng. File sẽ được di chuyển thẳng vào Thùng rác.
                                        </div>
                                    }>
                                        <div class="border rounded-xl overflow-hidden border-gray-200 dark:border-white/10">
                                            <div class="grid grid-cols-2 text-xs font-semibold py-1.5 px-3 bg-red-500/10 text-red-500 border-b border-gray-200 dark:border-white/10">
                                                <span>Tiến trình sẽ bị tắt</span>
                                                <span class="text-right">PID</span>
                                            </div>
                                            <For each={lockProcesses()}>
                                                {(proc) => (
                                                    <div class="grid grid-cols-2 text-xs py-2 px-3 hover:bg-black/5 dark:hover:bg-white/5 border-b last:border-0 border-gray-100 dark:border-white/5 font-mono">
                                                        <span class="truncate font-semibold text-gray-700 dark:text-gray-300">{proc.name}</span>
                                                        <span class="text-right text-gray-400">{proc.pid}</span>
                                                    </div>
                                                )}
                                            </For>
                                        </div>
                                    </Show>
                                </Show>
                            </div>

                            <div class="flex gap-2 mt-4">
                                <button 
                                    onClick={() => setShowForceDeleteModal(false)}
                                    class={`flex-1 py-2 rounded-lg text-xs font-medium border transition-colors
                                        ${isDark() ? 'bg-white/5 hover:bg-white/10 border-white/10' : 'bg-gray-100 hover:bg-gray-200 border-gray-200 text-gray-700'}`}
                                >
                                    Hủy
                                </button>
                                <button 
                                    onClick={handleForceDelete}
                                    class="flex-1 py-2 rounded-lg bg-red-600 hover:bg-red-700 text-white text-xs font-semibold shadow-lg shadow-red-600/25 transition-all active:scale-95"
                                >
                                    Đóng tiến trình & Xóa
                                </button>
                            </div>
                        </div>
                    </div>
                </Show>
            </div>
        </div>
    );
};
