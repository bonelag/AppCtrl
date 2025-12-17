// App execution type
export type AppType = 'exe' | 'bat' | 'shell';

// Application configuration
export interface AppConfig {
    id: string;
    name: string;
    icon: string | null;
    appType: AppType;
    executablePath: string;
    workingDirectory: string;
    arguments: string;
    environmentVars: string;
    isRunning: boolean;
}

// Modal state
export type ModalState =
    | { type: 'closed' }
    | { type: 'add' }
    | { type: 'edit'; appId: string }
    | { type: 'settings' };

// Settings
export interface AppSettings {
    minimizeToTray: boolean;
    theme: 'dark' | 'light';
}
