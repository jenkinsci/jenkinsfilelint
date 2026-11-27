pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def branch = env.CHANGE_BRANCH
                    sh "git checkout ${branch}"
                }
            }
        }
    }
}
