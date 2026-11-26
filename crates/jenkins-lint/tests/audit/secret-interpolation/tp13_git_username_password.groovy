pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([gitUsernamePassword(credentialsId: 'git', gitToolName: 'Default')]) {
                    sh "git push https://${GIT_USERNAME}:${GIT_PASSWORD}@github.com/org/repo.git"
                }
            }
        }
    }
}
