import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export function QuitWindow() {

    useEffect(() => {
        // Show window on mount
        const win = getCurrentWindow();
        win.show();
        win.setFocus();
    }, []);

    async function handleConfirm() {
        await invoke("upload_and_quit");
    }

    async function handleCancel() {
        // Hide window
        const win = getCurrentWindow();
        await win.hide();
    }

    return (
        <div className="h-screen bg-gray-50 flex flex-col items-center justify-center p-6 text-center">
            <div className="p-3 bg-blue-100 rounded-full mb-4">
                <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6 text-blue-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
                </svg>
            </div>
            <h3 className="text-xl font-bold text-gray-900 mb-2">Pending Uploads</h3>
            <p className="text-gray-600 mb-8 max-w-xs mx-auto">
                You have screenshots waiting to be uploaded. We will upload them before quitting.
            </p>

            <div className="flex gap-3 justify-center w-full">
                <button
                    onClick={handleCancel}
                    className="flex-1 max-w-[120px] px-4 py-2 text-sm font-semibold text-gray-700 bg-white border border-gray-300 hover:bg-gray-50 rounded-lg"
                >
                    Cancel
                </button>
                <button
                    onClick={handleConfirm}
                    className="flex-1 max-w-[160px] px-4 py-2 text-sm font-semibold text-white bg-blue-600 hover:bg-blue-700 rounded-lg shadow-sm"
                >
                    Upload & Quit
                </button>
            </div>
        </div>
    );
}
