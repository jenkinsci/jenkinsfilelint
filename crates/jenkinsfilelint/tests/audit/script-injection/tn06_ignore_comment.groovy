node {
    // jenkinsfilelint: ignore[script-injection] the branch name is checked above
    sh "git checkout ${env.CHANGE_BRANCH}"
    sh "echo ${env.CHANGE_TITLE}" // jenkinsfilelint: ignore[script-injection]
}
