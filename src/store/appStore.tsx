import { createContext, useContext, type ParentComponent, onMount } from 'solid-js';
import { createStore } from 'solid-js/store';
import { invoke } from '@tauri-apps/api/core';
import type { AppConfig, ModalState, AppSettings } from '../types';

interface AppStore {
    apps: AppConfig[];
    modal: ModalState;
    logs: { [appId: string]: string[] };
    settings: AppSettings;
}

interface ConfigData {
    apps?: AppConfig[];
    settings?: AppSettings;
}

interface AppActions {
    addApp: (app: Omit<AppConfig, 'id' | 'isRunning'>) => void;
    updateApp: (id: string, data: Partial<AppConfig>) => void;
    deleteApp: (id: string) => void;
    openAddModal: () => void;
    openEditModal: (appId: string) => void;
    openSettingsModal: () => void;
    closeModal: () => void;
    setAppRunning: (id: string, running: boolean) => void;
    appendLog: (appId: string, line: string) => void;
    clearLogs: (appId: string) => void;
    updateSettings: (settings: Partial<AppSettings>) => void;
    getApp: (id: string) => AppConfig | undefined;
}

const AppContext = createContext<[AppStore, AppActions]>();

function generateId(): string {
    return Math.random().toString(36).substr(2, 9);
}

export const AppProvider: ParentComponent = (props) => {
    const [store, setStore] = createStore<AppStore>({
        apps: [],
        modal: { type: 'closed' },
        logs: {},
        settings: { minimizeToTray: true, theme: 'dark' },
    });

    const saveConfig = async () => {
        const config: ConfigData = {
            apps: store.apps,
            settings: store.settings,
        };
        try {
            await invoke('save_config', { config: JSON.stringify(config, null, 2) });
        } catch (e) {
            console.error('Failed to save config:', e);
        }
    };

    onMount(async () => {
        try {
            const json = await invoke<string>('load_config');
            if (json && json.trim() !== '{}') {
                const config: ConfigData = JSON.parse(json);
                if (config.apps) setStore('apps', config.apps);
                if (config.settings) setStore('settings', config.settings);
            }
        } catch (e) {
            console.error('Failed to load config:', e);
        }
    });

    const actions: AppActions = {
        addApp: (appData) => {
            const newApp: AppConfig = {
                id: generateId(),
                ...appData,
                isRunning: false,
            };
            setStore('apps', (apps) => {
                const updated = [...apps, newApp];
                return updated;
            });
            saveConfig();
        },

        updateApp: (id, data) => {
            setStore('apps', (app) => app.id === id, data);
            saveConfig();
        },

        deleteApp: (id) => {
            setStore('apps', (apps) => {
                const updated = apps.filter((app) => app.id !== id);
                return updated;
            });
            saveConfig();
        },

        openAddModal: () => setStore('modal', { type: 'add' }),
        openEditModal: (appId) => setStore('modal', { type: 'edit', appId }),
        openSettingsModal: () => setStore('modal', { type: 'settings' }),
        closeModal: () => setStore('modal', { type: 'closed' }),

        setAppRunning: (id, running) => {
            setStore('apps', (app) => app.id === id, 'isRunning', running);
        },

        appendLog: (appId, line) => {
            setStore('logs', appId, (logs) => [...(logs || []), line]);
        },

        clearLogs: (appId) => {
            setStore('logs', appId, []);
        },

        updateSettings: (newSettings) => {
            setStore('settings', newSettings);
            saveConfig();
        },

        getApp: (id) => store.apps.find(a => a.id === id),
    };

    return (
        <AppContext.Provider value={[store, actions]}>
            {props.children}
        </AppContext.Provider>
    );
};

export function useApp() {
    const context = useContext(AppContext);
    if (!context) throw new Error('useApp must be used within AppProvider');
    return context;
}
