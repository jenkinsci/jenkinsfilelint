// An image that Groovy computes is not known to the audit.
def image = params.IMAGE ?: 'maven'

pipeline {
    agent { docker { image "${env.REGISTRY}/maven" } }
    stages {
        stage('Build') {
            steps {
                script {
                    docker.image(image).inside {
                        sh 'make'
                    }
                }
            }
        }
    }
}
