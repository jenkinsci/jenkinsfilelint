node {
    withEnv(["PR_TITLE=${env.CHANGE_TITLE}"]) {
        sh "echo ${env.PR_TITLE}"
        sh 'echo "$PR_TITLE"'
    }
    env.SOURCE = env.CHANGE_BRANCH
    sh "git fetch origin ${env.SOURCE}"
}
