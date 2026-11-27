node {
    sh "echo ${env.CHANGE_AUTHOR} ${env.CHANGE_FORK} ${env.CHANGE_AUTHOR_EMAIL}"
    sh "echo ${env.BRANCH_NAME} ${env.TAG_NAME} ${env.CHANGE_TARGET}"
}
