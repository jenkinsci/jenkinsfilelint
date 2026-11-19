pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                script {
                    if (count > env.LIMIT as Integer) {
                        echo 'too many'
                    }
                }
            }
        }
    }
}
