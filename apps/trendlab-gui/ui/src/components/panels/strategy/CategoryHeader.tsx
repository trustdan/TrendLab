import { VscChevronDown, VscChevronRight } from 'react-icons/vsc';

interface CategoryHeaderProps {
  name: string;
  selectedCount: number;
  totalCount: number;
  isExpanded: boolean;
  isFocused: boolean;
  onClick: () => void;
}

export function CategoryHeader({
  name,
  selectedCount,
  totalCount,
  isExpanded,
  isFocused,
  onClick,
}: CategoryHeaderProps) {
  const hasSelection = selectedCount > 0;

  return (
    <div
      className={`category-header ${isFocused ? 'focused' : ''}`}
      onClick={onClick}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onClick();
        }
      }}
    >
      <span className="category-expand-icon">
        {isExpanded ? <VscChevronDown /> : <VscChevronRight />}
      </span>
      <span className="category-name">{name}</span>
      <span className={`category-count ${hasSelection ? 'has-selection' : ''}`}>
        ({selectedCount}/{totalCount})
      </span>

      <style>{`
        .category-header {
          display: flex;
          align-items: center;
          gap: var(--space-sm);
          padding: var(--space-xs) var(--space-sm);
          cursor: pointer;
          border-radius: var(--radius-sm);
          user-select: none;
        }

        .category-header:hover {
          background: var(--bg-hover);
        }

        .category-header.focused {
          background: var(--bg-active);
          outline: 1px solid var(--yellow);
        }

        .category-expand-icon {
          display: flex;
          align-items: center;
          color: var(--muted);
        }

        .category-name {
          font-weight: 600;
          color: var(--cyan);
        }

        .category-header.focused .category-name {
          color: var(--yellow);
        }

        .category-count {
          color: var(--muted);
          font-size: var(--font-size-sm);
        }

        .category-count.has-selection {
          color: var(--green);
        }
      `}</style>
    </div>
  );
}
