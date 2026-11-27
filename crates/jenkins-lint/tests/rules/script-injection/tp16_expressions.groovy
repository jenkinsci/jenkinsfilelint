node {
    sh "docker tag app app:${env.CHANGE_BRANCH.toLowerCase()}"
    sh "echo ${env.CHANGE_BRANCH - 'feature/'}"
    sh "echo ${env.CHANGE_TITLE ?: 'none'}"
    sh "echo ${env.CHANGE_TITLE.take(20)}"
    sh "echo ${env.CHANGE_TITLE.size() > 20 ? env.CHANGE_TITLE.take(20) : env.CHANGE_TITLE}"
}
