node {
    sh "echo \"\$CHANGE_TITLE\" in ${env.WORKSPACE}"
    sh """
        git checkout "\${CHANGE_BRANCH}"
    """
}
