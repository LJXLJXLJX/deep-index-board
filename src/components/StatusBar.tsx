import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";

export const StatusBar: React.FC = () => {
  const [memory, setMemory] = useState<number>(0);
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [isAutostartLoading, setIsAutostartLoading] = useState(true);

  const fetchMemory = async () => {
    try {
      const bytes = await invoke<number>("get_memory_usage");
      setMemory(bytes);
    } catch (e) {
      console.error("Failed to fetch memory usage:", e);
    }
  };

  useEffect(() => {
    fetchMemory();
    const timer = setInterval(fetchMemory, 5000);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    const fetchAutostart = async () => {
      try {
        setAutostartEnabled(await isEnabled());
      } catch (e) {
        console.error("Failed to fetch autostart state:", e);
      } finally {
        setIsAutostartLoading(false);
      }
    };

    fetchAutostart();
  }, []);

  const formatMemory = (bytes: number) => {
    if (bytes === 0) return "Loading...";
    const mb = bytes / (1024 * 1024);
    if (mb < 1024) {
      return `${mb.toFixed(1)} MB`;
    }
    return `${(mb / 1024).toFixed(2)} GB`;
  };

  const toggleAutostart = async () => {
    const nextEnabled = !autostartEnabled;
    setIsAutostartLoading(true);
    try {
      if (nextEnabled) {
        await enable();
      } else {
        await disable();
      }
      setAutostartEnabled(nextEnabled);
    } catch (e) {
      console.error("Failed to update autostart state:", e);
    } finally {
      setIsAutostartLoading(false);
    }
  };

  return (
    <footer className="status-bar">
      <div className="status-left" />

      <div className="status-right">
        <label className="autostart-toggle">
          <input
            type="checkbox"
            checked={autostartEnabled}
            disabled={isAutostartLoading}
            onChange={toggleAutostart}
          />
          <span className="toggle-track">
            <span className="toggle-thumb" />
          </span>
          <span>开机自启动</span>
        </label>

        <div className="status-item">
          <span>内存占用：</span>
          <span className="mem-info">{formatMemory(memory)}</span>
        </div>
      </div>
    </footer>
  );
};
