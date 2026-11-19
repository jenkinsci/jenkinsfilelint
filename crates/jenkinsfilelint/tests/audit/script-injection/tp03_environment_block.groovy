pipeline {
    agent any
    environment {
        TITLE = "${env.CHANGE_TITLE}"
        SLUG = "pr-${env.CHANGE_BRANCH}"
    }
    stages {
        stage('Report') {
            steps {
                sh "echo ${TITLE}"
                sh "deploy --name ${env.SLUG}"
                sh 'echo "$TITLE"'
            }
        }
    }
}
