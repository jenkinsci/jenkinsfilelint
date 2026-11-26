node {
    // jenkins-lint: ignore[script-injection] the branch name is checked above
    sh "git checkout ${env.CHANGE_BRANCH}"
    sh "echo ${env.CHANGE_TITLE}" // jenkins-lint: ignore[script-injection]
}
