"use client";

import React, { useEffect, useState } from "react";
import { cn } from "../../lib/utils";

interface SkinViewerProps {
  skinUrl: string; // This will now be the direct URL (file:// or http:// or /path)
  playerName?: string;
  width?: number;
  height?: number;
  className?: string;
  style?: React.CSSProperties;
}

export function SkinViewer({
  skinUrl, // Directly use this prop
  playerName,
  width = 300,
  height = 400,
  className,
  style,
}: SkinViewerProps) {
  const [hasError, setHasError] = useState(false);

  // Reset error state if skinUrl changes, to allow retrying if a new valid URL is provided
  useEffect(() => {
    setHasError(false);
  }, [skinUrl]);

  const handleError = () => {
    console.warn(`[SkinViewer] Error loading image from skinUrl: ${skinUrl}`);
    setHasError(true);
  };

  if (hasError || !skinUrl) {
    // Show fallback if error or no skinUrl provided
    return (
      <div
        className={cn(
          "flex items-center justify-center bg-gray-700/50 rounded-md",
          className,
        )}
        style={{ width, height, ...style }}
      >
        <span className="text-gray-500 text-3xl">?</span>
      </div>
    );
  }

  return (
    <img
      src={skinUrl}
      alt={playerName ? `${playerName}'s Skin` : "Minecraft Skin"}
      width={width}
      height={height}
      className={cn("object-contain rounded-md select-none", className)}
      style={{
        imageRendering: "pixelated",
        userSelect: "none",
        ...style,
      }}
      draggable={false}
      onError={handleError}
    />
  );
}
