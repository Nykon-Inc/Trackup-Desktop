import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export function IdleModal() {
  const [idleTime, setIdleTime] = useState<number | null>(null);

  useEffect(() => {
    const unlisten = listen<number>("idle_ended", (event) => {
      // Only show modal if the idle time was significant (> 5 mins which is 300s)
      // The backend emits idle_ended ONLY if it was in idle mode (>= 300s).
      // So we can trust the event.
      setIdleTime(event.payload);

      // Bring window to front? (Optional, might be annoying if they are doing something else)
    });

    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  async function handleChoice(keep: boolean) {
    if (idleTime === null) return;
    try {
      await invoke("process_idle_choice", { idleTime, keep });
      setIdleTime(null);
    } catch (err) {
      console.error("Failed to process idle choice", err);
    }
  }

  if (idleTime === null) return null;

  const minutes = Math.floor(idleTime / 60);
  const seconds = idleTime % 60;

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 backdrop-blur-sm">
      <div className="bg-white rounded-xl shadow-2xl p-6 w-md transform transition-all">
        <div className="flex items-start gap-4">
          <div className="p-3 bg-yellow-100 rounded-full shrink-0">
            <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6 text-yellow-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
          </div>
          <div className="flex-1">
            <h3 className="text-lg font-bold text-gray-900">You were away</h3>
            <p className="mt-2 text-gray-600 leading-relaxed">
              We detected no activity for <span className="font-bold text-gray-900">{minutes} minutes and {seconds} seconds</span>.
              Would you like to include this time in your session?
            </p>
          </div>
        </div>

        <div className="mt-8 flex gap-3 justify-end">
          <button
            onClick={() => handleChoice(false)}
            className="px-4 py-2.5 text-sm font-semibold text-gray-700 bg-white border border-gray-300 hover:bg-gray-50 rounded-lg transition-colors focus:ring-2 focus:ring-gray-200"
          >
            Discard Time
          </button>
          <button
            onClick={() => handleChoice(true)}
            className="px-4 py-2.5 text-sm font-semibold text-white bg-blue-600 hover:bg-blue-700 rounded-lg shadow-sm transition-colors focus:ring-2 focus:ring-blue-100"
          >
            Keep Time
          </button>
        </div>
      </div>
    </div>
  );
}
