node {
    def branch = env.CHANGE_BRANCH
    branch = 'main'
    sh "git checkout ${branch}"
    if (true) {
        def CHANGE_TITLE = 'fixed'
        sh "echo ${CHANGE_TITLE}"
    }
}
