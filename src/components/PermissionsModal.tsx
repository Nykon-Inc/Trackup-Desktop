import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Permissions {
    accessibility: boolean;
    screenRecording: boolean;
}

export function PermissionsModal({ onGranted }: { onGranted: () => void }) {
    const [permissions, setPermissions] = useState<Permissions>({
        accessibility: false,
        screenRecording: false,
    });
    const initialized = useRef(false);

    const checkPermissions = async () => {
        try {
            const res = await invoke<Permissions>("check_permissions");
            console.log(res)
            setPermissions(res);
            if (res.accessibility && res.screenRecording) {
                onGranted();
            }
        } catch (err) {
            console.error("Failed to check permissions", err);
        }
    };

    useEffect(() => {
        if (!initialized.current) {
            initialized.current = true;
            checkPermissions();
        }
        // Poll for changes as macOS settings change doesn't always trigger app events
        const interval = setInterval(checkPermissions, 1500);
        return () => clearInterval(interval);
    }, []);

    const openSettings = async (type: string) => {
        console.log("Calling open_permissions_settings with type:", type);
        try {
            await invoke("open_permissions_settings", { typeName: type });
        } catch (err) {
            console.error("Failed to open settings:", err);
        }
    };

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
            {/* Heavy Backdrop */}
            <div className="absolute inset-0 bg-gray-900/60 backdrop-blur-md" />

            {/* Modal Container */}
            <div className="relative bg-white/95 backdrop-blur-md rounded-2xl shadow-xl max-w-[340px] w-full overflow-hidden animate-in fade-in zoom-in duration-500 border border-white/20">
                <div className="p-7">
                    <div className="flex flex-col items-center text-center mb-6">
                        <div className="w-12 h-12 bg-primary/10 rounded-xl flex items-center justify-center mb-3">
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6 text-primary" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.5} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                            </svg>
                        </div>
                        <h2 className="text-xl font-bold text-gray-900 tracking-tight">System Permissions</h2>
                        <p className="text-gray-500 mt-1 text-[11px] leading-normal">
                            StaffWatch requires these permissions to track your activity and provide accurate work logs.
                        </p>
                    </div>

                    <div className="space-y-2.5">
                        <PermissionItem
                            title="Accessibility"
                            description="Track window titles and idle time."
                            granted={permissions.accessibility}
                            onAction={() => openSettings('accessibility')}
                        />
                        <PermissionItem
                            title="Screen Recording"
                            description="Capturing periodic screenshots."
                            granted={permissions.screenRecording}
                            onAction={() => openSettings('screenRecording')}
                        />
                    </div>

                    <div className="mt-6 pt-5 border-t border-gray-100">
                        <div className="flex items-start gap-2.5 text-[10px] text-gray-400 bg-gray-50/80 p-3 rounded-lg leading-tight italic">
                            <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4 shrink-0 text-gray-300 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                            </svg>
                            <span>If permissions are granted but not detected, you may need to restart the app.</span>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    );
}

function PermissionItem({ title, description, granted, onAction }: { title: string; description: string; granted: boolean; onAction: () => void }) {
    return (
        <div className={`group p-3 rounded-xl border transition-all duration-300 ${granted ? 'border-green-100 bg-green-50/40' : 'border-gray-100 bg-white hover:border-primary/20 hover:shadow-sm'}`}>
            <div className="flex items-center justify-between gap-3">
                <div className="flex-1">
                    <div className="flex items-center gap-2">
                        <span className={`text-sm font-semibold ${granted ? 'text-green-700' : 'text-gray-800'}`}>{title}</span>
                        {granted && (
                            <span className="bg-green-100 text-green-700 text-[9px] px-1.5 py-0.5 rounded font-bold uppercase tracking-wider">Active</span>
                        )}
                    </div>
                    <p className="text-[10px] text-gray-500 mt-0.5 leading-tight">{description}</p>
                </div>

                {granted ? (
                    <div className="bg-green-500 text-white rounded-full p-1 shadow-sm">
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={3.5}>
                            <polyline points="20 6 9 17 4 12" />
                        </svg>
                    </div>
                ) : (
                    <button
                        onClick={onAction}
                        className="bg-primary text-white px-3.5 py-1.5 rounded-lg text-[10px] font-bold hover:shadow-md hover:opacity-90 active:scale-95 transition-all cursor-pointer whitespace-nowrap"
                    >
                        Enable
                    </button>
                )}
            </div>
        </div>
    );
}
