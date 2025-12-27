import { useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../../../store';
import { useDebounce } from '../../../hooks';
import type { SearchResult } from '../../../types';

/** Debounce delay in milliseconds */
const SEARCH_DEBOUNCE_MS = 300;

/** Minimum query length to trigger search */
const MIN_QUERY_LENGTH = 2;

export function SearchOverlay() {
  const inputRef = useRef<HTMLInputElement>(null);
  const abortControllerRef = useRef<AbortController | null>(null);

  const searchQuery = useAppStore((s) => s.searchQuery);
  const searchResults = useAppStore((s) => s.searchResults);
  const searchLoading = useAppStore((s) => s.searchLoading);
  const searchSelectedIndex = useAppStore((s) => s.searchSelectedIndex);
  const setSearchQuery = useAppStore((s) => s.setSearchQuery);
  const setSearchResults = useAppStore((s) => s.setSearchResults);
  const setSearchLoading = useAppStore((s) => s.setSearchLoading);
  const exitSearchMode = useAppStore((s) => s.exitSearchMode);
  const navigateSearchResult = useAppStore((s) => s.navigateSearchResult);
  const selectSearchResult = useAppStore((s) => s.selectSearchResult);

  // Debounce the search query for better performance
  const debouncedQuery = useDebounce(searchQuery, SEARCH_DEBOUNCE_MS);

  // Focus input on mount
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Show loading indicator immediately when typing (before debounce completes)
  useEffect(() => {
    if (searchQuery.length >= MIN_QUERY_LENGTH && searchQuery !== debouncedQuery) {
      setSearchLoading(true);
    }
  }, [searchQuery, debouncedQuery, setSearchLoading]);

  // Execute search when debounced query changes
  useEffect(() => {
    // Clear results if query is too short
    if (debouncedQuery.length < MIN_QUERY_LENGTH) {
      setSearchResults([]);
      setSearchLoading(false);
      return;
    }

    // Cancel previous request to prevent race conditions
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }

    const controller = new AbortController();
    abortControllerRef.current = controller;

    const executeSearch = async () => {
      setSearchLoading(true);
      try {
        const results = await invoke<SearchResult[]>('search_symbols', {
          query: debouncedQuery,
        });
        if (!controller.signal.aborted) {
          setSearchResults(results);
          setSearchLoading(false);
        }
      } catch (err) {
        if (!controller.signal.aborted) {
          console.error('Search failed:', err);
          setSearchResults([]);
          setSearchLoading(false);
        }
      }
    };

    executeSearch();

    return () => {
      controller.abort();
    };
  }, [debouncedQuery, setSearchLoading, setSearchResults]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      switch (e.key) {
        case 'Escape':
          e.preventDefault();
          exitSearchMode();
          break;
        case 'ArrowDown':
        case 'j':
          if (e.key === 'j' && e.target === inputRef.current) break;
          e.preventDefault();
          navigateSearchResult(1);
          break;
        case 'ArrowUp':
        case 'k':
          if (e.key === 'k' && e.target === inputRef.current) break;
          e.preventDefault();
          navigateSearchResult(-1);
          break;
        case 'Enter':
          e.preventDefault();
          selectSearchResult();
          break;
      }
    },
    [exitSearchMode, navigateSearchResult, selectSearchResult]
  );

  return (
    <div className="search-overlay" onKeyDown={handleKeyDown}>
      <div className="search-header">
        <input
          ref={inputRef}
          type="text"
          className="search-input"
          placeholder="Search for symbols..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
        />
        <button className="search-close" onClick={exitSearchMode}>
          Esc
        </button>
      </div>

      <div className="search-results">
        {searchLoading && (
          <div className="search-loading">Searching...</div>
        )}
        {!searchLoading && searchResults.length === 0 && searchQuery.length >= 2 && (
          <div className="search-empty">No results found</div>
        )}
        {!searchLoading && searchResults.map((result, index) => (
          <div
            key={result.symbol}
            className={`search-result ${index === searchSelectedIndex ? 'selected' : ''}`}
            onClick={() => {
              navigateSearchResult(index - searchSelectedIndex);
              selectSearchResult();
            }}
          >
            <span className="result-indicator">
              {index === searchSelectedIndex ? '▸' : ' '}
            </span>
            <span className="result-symbol">{result.symbol}</span>
            <span className="result-name">{result.name}</span>
            <span className="result-type">
              ({result.type_disp}/{result.exchange})
            </span>
          </div>
        ))}
      </div>

      <div className="search-help">
        <span>Enter: Select</span>
        <span>Esc: Cancel</span>
        <span>↑↓: Navigate</span>
      </div>

      <style>{`
        .search-overlay {
          display: flex;
          flex-direction: column;
          height: 100%;
          background: var(--bg);
          border-left: 1px solid var(--blue);
        }
        .search-header {
          display: flex;
          gap: var(--space-sm);
          padding: var(--space-md);
          border-bottom: 1px solid var(--border);
          background: var(--bg-secondary);
        }
        .search-input {
          flex: 1;
          padding: var(--space-sm) var(--space-md);
          background: var(--bg);
          border: 1px solid var(--border);
          border-radius: var(--radius-sm);
          color: var(--fg);
          font-family: var(--font-mono);
          font-size: var(--font-size-sm);
        }
        .search-input:focus {
          outline: none;
          border-color: var(--blue);
        }
        .search-close {
          padding: var(--space-sm) var(--space-md);
          background: none;
          border: 1px solid var(--border);
          border-radius: var(--radius-sm);
          color: var(--muted);
          cursor: pointer;
          font-family: var(--font-mono);
          font-size: var(--font-size-xs);
        }
        .search-close:hover {
          background: var(--bg-secondary);
          color: var(--fg);
        }
        .search-results {
          flex: 1;
          overflow-y: auto;
          padding: var(--space-sm) 0;
        }
        .search-loading,
        .search-empty {
          padding: var(--space-md);
          color: var(--muted);
          text-align: center;
        }
        .search-result {
          display: flex;
          align-items: center;
          gap: var(--space-sm);
          padding: var(--space-xs) var(--space-md);
          cursor: pointer;
          font-family: var(--font-mono);
          font-size: var(--font-size-sm);
        }
        .search-result:hover {
          background: var(--bg-secondary);
        }
        .search-result.selected {
          background: var(--blue-bg);
        }
        .result-indicator {
          color: var(--yellow);
          width: 1ch;
        }
        .result-symbol {
          color: var(--cyan);
          font-weight: 600;
          min-width: 60px;
        }
        .result-name {
          flex: 1;
          color: var(--fg);
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
        }
        .result-type {
          color: var(--muted);
          font-size: var(--font-size-xs);
        }
        .search-help {
          display: flex;
          gap: var(--space-md);
          padding: var(--space-sm) var(--space-md);
          border-top: 1px solid var(--border);
          background: var(--bg-secondary);
          color: var(--muted);
          font-family: var(--font-mono);
          font-size: var(--font-size-xs);
        }
      `}</style>
    </div>
  );
}
