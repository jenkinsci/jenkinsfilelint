pipeline {
    agent any
    stages {
        stage('Push') {
            steps {
                script {
                    def branch = env.CHANGE_BRANCH
                    sh "git push origin ${branch}"
                    def cmd = "git tag ${branch}-rc"
                    sh cmd
                }
            }
        }
    }
}
