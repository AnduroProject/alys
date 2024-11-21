#!/usr/bin/env bash
# Script to create a commit

echo "Auto Commit Script"
echo "=================="

GIT_STAGE_ALL=false
GIT_AUTO_PUSH=false
GIT_COMMIT_MESSAGE=""
GIT_COMMIT_TYPE=""
GIT_NO_VERIFY=false
BREAKING_CHANGE=""

COMMIT_BODY=""

while getopts 'aphvcm:' OPTION; do
  case "$OPTION" in
    a)
        GIT_STAGE_ALL=true
        ;;
    p)
        GIT_AUTO_PUSH=true
        ;;
    h)
        echo "Usage: ./commit.sh [-a] [-p] [-m <commit message>]"
        echo " -a: stage all files"
        echo " -p: push commit to remote"
        echo " -m: commit message"
        echo " -c: check pre-commit hook"
        echo " -v: skip git commit verification"
        exit 0
        ;;
    m)
        GIT_COMMIT_MESSAGE="$OPTARG"
        ;;
    v)
        GIT_NO_VERIFY=true
        ;;
    c)
        if [ -d ".git" ]; then
            ./.git/hooks/pre-commit
        elif [ -d "../.git" ]; then
            PROJECT_NAME=$(basename "$PWD")
            PRE_COMMIT_HOOK_PATH="../.git/modules/${PROJECT_NAME}/hooks/pre-commit"
            if [ -f "$PRE_COMMIT_HOOK_PATH" ]; then
                $PRE_COMMIT_HOOK_PATH
            else
                echo "No pre-commit hook found."
                exit 1
            fi

        else
            echo "No .git directory found"
            exit 1
        fi
        exit 0
        ;;
    *)
        echo " "
        echo "Unsupported option: -$OPTION"
        echo "Use -h for help"
        exit 1
  esac
done

if [[ -z $(git diff --cached --exit-code) ]]; then
    if [[ -z $(git status -s) ]]; then
        echo "Your tree is clean. Make some changes and stage them first."
        exit 0
    else
        if [ "$GIT_STAGE_ALL" == "true" ]; then
            git add -A
            echo "Automatically staged all your changes"
        else
            echo -e "\nPlease stage your changes before committing them.\nYou can do this by running 'git add -u' or 'git add -A' to stage all files, or 'git add <file>' to stage a specific file.\n\nTo automatically stage all files, pass the flag '-a'.\n"
            exit 1
        fi
    fi
fi

echo " "
echo "Tip:"
echo "Use ENTER / RETURN to continue"
echo "Use CTRL+C / ^+C to exit"
echo " "


echo -e "\nCommit Types"

PS3="$(echo -e "\nChoose a commit type: ")"
select GIT_COMMIT_TYPE in "feat ✨" "fix 🐛" "refactor 📦" "review 👌" "model 📐" "revert 🔙" "perf 🚀" "chore 🎫" "test 🧯" "docs 📖" "hotfix 🚑" "security 🔒" "style💎" "build 👷" "ci 💻" "upgrade ⬆"️ "downgrade ⬇"️ "deployment 🌈" "release 🔖"
do
    break
done

echo -e "Selected commit type: $GIT_COMMIT_TYPE"

yesexp="^[yY][eE][sS]|[tT][rR][uU][eE]|[yY]$"
noexp="^[nN][oO]|[fF][aA][lL][sS][eE]|[nN]$"

while true; do
    echo ""
    read -p "Are there any breaking changes (yes/no)? " BC
    if [[ "$BC" =~ $yesexp ]]; then BREAKING_CHANGE="!"; break; fi
    if [[ "$BC" =~ $noexp ]]; then break; fi
    echo "Answer yes/no. '$BC' is not a valid answer."
done


  if [ "$GIT_COMMIT_MESSAGE" == "" ]; then
    while true; do
      echo " "
      read -p "Write a short description of the change: " GIT_COMMIT_MESSAGE
      if [ "$GIT_COMMIT_MESSAGE" == "" ]; then
        echo "You must provide a description."
      else
        break
      fi
    done
  fi

REQUIRE_DESCRIPTION="optional"
if [ "$BREAKING_CHANGE" == "!" ]; then
  REQUIRE_DESCRIPTION="required"
fi

while true; do
    echo ""
    read -p "Provide a longer description of the change ($REQUIRE_DESCRIPTION): " BODY
    if [ "$BREAKING_CHANGE" == "!" ] && [ "$BODY" == "" ]; then
        echo "You must provide a longer description if there are breaking changes."
    else
        break
    fi
done

BRANCH="$(git rev-parse --abbrev-ref HEAD)"
ISSUE=$(echo "$BRANCH" | awk 'match($0, /(^|\/)[0-9]+-/) {print substr($0, RSTART + (substr($0, RSTART, 1) == "/" ? 1 : 0), RLENGTH - 1)}')

if [ "$ISSUE" == "" ]; then
    echo -e "\n\n ⚠️Could not find a issue number in the branch name. Please make sure the branch name is in the format '<type>/<issue-number>-<short-description>' or '<issue-number>-<issue-name>'.\n"
else
    ISSUE_DESCRIPTION="(press ENTER / RETURN to set $ISSUE) "
fi

while true; do
    echo ""
    read -p "Provide a GitHub Issue number: $ISSUE_DESCRIPTION" GITHUB_ISSUE
    if [ "$GITHUB_ISSUE" == "" ]; then
        GITHUB_ISSUE=$ISSUE
        break
    else
        break
    fi
done

COMMIT_BODY=$(echo -e "$GIT_COMMIT_TYPE$BREAKING_CHANGE: $GIT_COMMIT_MESSAGE\n\n$BODY\n\nISSUE: #$GITHUB_ISSUE\n")

echo -e "\n============================================================\n"

if [ "$GIT_NO_VERIFY" == "true" ]; then
    git commit --no-verify -S -m "$COMMIT_BODY"
else
    git commit -S -m "$COMMIT_BODY"
fi

if [ "$GIT_AUTO_PUSH" == "true" ]; then
    git push origin "$BRANCH"
fi
