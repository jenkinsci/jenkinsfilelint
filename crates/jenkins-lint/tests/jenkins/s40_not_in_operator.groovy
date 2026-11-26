pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                script {
                    if (env.BRANCH !in ['main']) {
                        echo 'feature'
                    }
                }
            }
        }
    }
}
