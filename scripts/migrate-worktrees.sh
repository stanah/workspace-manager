#!/usr/bin/env bash
# migrate-worktrees.sh
# Parallel スタイル ({repo}__{branch}) → Subdirectory スタイル ({repo}/.worktrees/{branch}) へ移行
#
# Usage:
#   ./scripts/migrate-worktrees.sh              # dry-run (変更なし)
#   ./scripts/migrate-worktrees.sh --execute    # 実行

set -euo pipefail

DRY_RUN=true
if [[ "${1:-}" == "--execute" ]]; then
    DRY_RUN=false
fi

# Parallel スタイルの worktree を検索 ({name}__{branch} パターン)
find_parallel_worktrees() {
    local search_dir="$1"
    find "$search_dir" -maxdepth 4 -name ".git" -type f 2>/dev/null | while read -r gitfile; do
        local dir
        dir=$(dirname "$gitfile")
        local basename
        basename=$(basename "$dir")

        # __{branch} パターンにマッチするか
        if [[ "$basename" == *__* ]]; then
            echo "$dir"
        fi
    done
}

migrate_worktree() {
    local old_path="$1"
    local basename
    basename=$(basename "$old_path")

    # {repo}__{branch} からリポジトリ名とブランチ名を抽出
    local repo_name="${basename%%__*}"
    local branch_name="${basename#*__}"
    local parent_dir
    parent_dir=$(dirname "$old_path")
    local repo_path="${parent_dir}/${repo_name}"

    # リポジトリが存在するか確認
    if [[ ! -d "$repo_path/.git" ]] && [[ ! -f "$repo_path/.git" ]]; then
        echo "  SKIP: リポジトリが見つかりません: $repo_path"
        return 1
    fi

    local new_path="${repo_path}/.worktrees/${branch_name}"

    echo "  FROM: $old_path"
    echo "    TO: $new_path"

    if [[ "$DRY_RUN" == true ]]; then
        echo "  (dry-run: 変更なし)"
        return 0
    fi

    # .worktrees ディレクトリを作成
    mkdir -p "${repo_path}/.worktrees"

    # .gitignore に .worktrees を追加（未登録の場合）
    local gitignore="${repo_path}/.gitignore"
    if [[ -f "$gitignore" ]]; then
        if ! grep -qxF '.worktrees/' "$gitignore" 2>/dev/null; then
            echo '.worktrees/' >> "$gitignore"
            echo "  ADDED: .worktrees/ を $gitignore に追加"
        fi
    else
        echo '.worktrees/' > "$gitignore"
        echo "  CREATED: $gitignore に .worktrees/ を追加"
    fi

    # git worktree move で安全に移動
    if git -C "$repo_path" worktree move "$old_path" "$new_path" 2>/dev/null; then
        echo "  OK: git worktree move 完了"
    else
        # git worktree move が失敗した場合（古い git バージョン等）
        # 手動で移動: ディレクトリ移動 + gitdir/worktree パスを更新
        echo "  WARN: git worktree move 失敗、手動で移行します"

        # gitdir リンクからワークツリーの内部パスを取得
        local gitdir
        gitdir=$(cat "${old_path}/.git" | sed 's/^gitdir: //')

        # ディレクトリを移動
        mv "$old_path" "$new_path"

        # .git ファイルの gitdir パスは変わらない（同じリポジトリ内なので）
        # ただし git 内部の worktree 参照を更新
        local worktree_link="${gitdir}/gitdir"
        if [[ -f "$worktree_link" ]]; then
            echo "${new_path}/.git" > "$worktree_link"
            echo "  OK: gitdir 参照を更新"
        fi

        echo "  OK: 手動移行完了"
    fi
}

echo "=== Worktree Migration: Parallel → Subdirectory ==="
echo ""

if [[ "$DRY_RUN" == true ]]; then
    echo "MODE: dry-run (--execute で実行)"
else
    echo "MODE: EXECUTE"
fi
echo ""

# 検索パスを config から取得、なければデフォルト
SEARCH_PATHS=("${HOME}/work" "${HOME}/ghq")

total=0
migrated=0
skipped=0

for search_path in "${SEARCH_PATHS[@]}"; do
    if [[ ! -d "$search_path" ]]; then
        continue
    fi

    echo "--- Scanning: $search_path ---"

    while IFS= read -r worktree_dir; do
        [[ -z "$worktree_dir" ]] && continue
        total=$((total + 1))

        echo ""
        echo "[$total] $(basename "$worktree_dir")"
        if migrate_worktree "$worktree_dir"; then
            migrated=$((migrated + 1))
        else
            skipped=$((skipped + 1))
        fi
    done < <(find_parallel_worktrees "$search_path")
done

echo ""
echo "=== 結果 ==="
echo "  検出: $total"
echo "  移行: $migrated"
echo "  スキップ: $skipped"

if [[ "$DRY_RUN" == true ]] && [[ $total -gt 0 ]]; then
    echo ""
    echo "実行するには: $0 --execute"
fi
