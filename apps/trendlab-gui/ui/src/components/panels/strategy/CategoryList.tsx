import { useAppStore } from '../../../store';
import { CategoryHeader } from './CategoryHeader';
import { StrategyCheckbox } from './StrategyCheckbox';
import { EnsembleToggle } from './EnsembleToggle';

interface CategoryListProps {
  isFocused: boolean;
}

export function CategoryList({ isFocused }: CategoryListProps) {
  const {
    categories,
    focusedCategoryIndex,
    focusedStrategyIndex,
    isCategoryExpanded,
    isStrategySelected,
    getSelectedCountForCategory,
    toggleCategoryExpanded,
    toggleStrategy,
    focus,
  } = useAppStore();

  const isSelectionFocused = focus === 'selection' && isFocused;

  return (
    <div className="category-list">
      <div className="category-list-header">
        <span className="list-title">Strategies</span>
        <span className="keyboard-hint">Space: toggle | a/n: all/none</span>
      </div>

      <div className="category-list-content">
        {categories.map((category, catIndex) => {
          const isExpanded = isCategoryExpanded(category.id);
          const selectedCount = getSelectedCountForCategory(category.id);
          const isCatFocused =
            isSelectionFocused &&
            focusedCategoryIndex === catIndex &&
            focusedStrategyIndex === -1;

          return (
            <div key={category.id} className="category-group">
              <CategoryHeader
                name={category.name}
                selectedCount={selectedCount}
                totalCount={category.strategies.length}
                isExpanded={isExpanded}
                isFocused={isCatFocused}
                onClick={() => toggleCategoryExpanded(category.id)}
              />

              {isExpanded && (
                <div className="strategy-list">
                  {category.strategies.map((strategy, stratIndex) => {
                    const isStratFocused =
                      isSelectionFocused &&
                      focusedCategoryIndex === catIndex &&
                      focusedStrategyIndex === stratIndex;

                    return (
                      <StrategyCheckbox
                        key={strategy.id}
                        name={strategy.name}
                        isSelected={isStrategySelected(strategy.id)}
                        isFocused={isStratFocused}
                        hasParams={strategy.has_params}
                        onClick={() => toggleStrategy(strategy.id)}
                      />
                    );
                  })}
                </div>
              )}
            </div>
          );
        })}
      </div>

      <div className="category-list-footer">
        <EnsembleToggle />
      </div>

      <style>{`
        .category-list {
          display: flex;
          flex-direction: column;
          height: 100%;
          border-right: 1px solid var(--border);
        }

        .category-list-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: var(--space-sm) var(--space-md);
          border-bottom: 1px solid var(--border);
        }

        .list-title {
          font-weight: 600;
          color: var(--fg);
        }

        .keyboard-hint {
          font-size: var(--font-size-xs);
          color: var(--muted);
        }

        .category-list-content {
          flex: 1;
          overflow-y: auto;
          padding: var(--space-sm);
        }

        .category-group {
          margin-bottom: var(--space-xs);
        }

        .strategy-list {
          margin-left: var(--space-sm);
        }

        .category-list-footer {
          border-top: 1px solid var(--border);
          padding: var(--space-sm) var(--space-md);
        }
      `}</style>
    </div>
  );
}
